// SPDX-FileCopyrightText: 2025 Russ Fellows <russ.fellows@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later

use anyhow::{Context, Result};
use futures_util::StreamExt;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tracing::{debug, error, info, warn, trace};

use crate::dlio_compat::DlioConfig;
use crate::metrics::Metrics;
use crate::plugins::PluginManager;

// Import s3dlio 0.8.0 functionality - using new advanced API
use s3dlio::api::advanced::{AsyncPoolDataLoader, MultiBackendDataset, PoolConfig};
use s3dlio::object_store::{store_for_uri, ObjectStore};
use s3dlio::{LoaderOptions, ReaderMode, LoadingMode, LoadBalanceStrategy, MultiEndpointStore};

/// Main workload execution engine using s3dlio capabilities
pub struct WorkloadRunner {
    config: Arc<DlioConfig>,
    metrics: Arc<Metrics>,
    accelerators: u32,
    strict_au: bool,
    rank: u32,
    world_size: u32,
    file_list: Option<Vec<String>>,
    plugins: Option<PluginManager>,
    checkpoint_state: Option<crate::plugins::checkpoint::CheckpointState>,
    // v0.8.5: Keep typed reference to MultiEndpointStore for stats access
    multi_endpoint_store: Option<Arc<MultiEndpointStore>>,
    // v0.8.6: Live performance statistics tracking
    live_ops: Arc<AtomicU64>,
    live_bytes: Arc<AtomicU64>,
}

/// Spawn a background task to monitor and display live performance statistics
/// 
/// This task updates the progress bar message every 0.5 seconds with:
/// - Operations per second (ops/s)
/// - Throughput in MiB/s
/// - Average latency in milliseconds
/// 
/// The monitor exits when the progress bar reaches completion.
fn spawn_live_stats_monitor(
    pb: indicatif::ProgressBar,
    ops_counter: Arc<AtomicU64>,
    bytes_counter: Arc<AtomicU64>,
    concurrency: usize,
    total_items: u64,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut last_ops = 0u64;
        let mut last_bytes = 0u64;
        let mut last_time = Instant::now();
        
        loop {
            tokio::time::sleep(Duration::from_millis(100)).await;
            
            // Exit when all items processed
            if pb.position() >= total_items {
                break;
            }
            
            let elapsed = last_time.elapsed();
            if elapsed.as_secs_f64() >= 0.5 {
                let current_ops = ops_counter.load(Ordering::Relaxed);
                let current_bytes = bytes_counter.load(Ordering::Relaxed);
                
                let ops_delta = current_ops.saturating_sub(last_ops);
                let bytes_delta = current_bytes.saturating_sub(last_bytes);
                let time_delta = elapsed.as_secs_f64();
                
                if ops_delta > 0 {
                    let ops_per_sec = ops_delta as f64 / time_delta;
                    let mib_per_sec = (bytes_delta as f64 / 1_048_576.0) / time_delta;
                    
                    // Estimate average latency (rough approximation)
                    let avg_latency_ms = if concurrency > 0 {
                        (time_delta * 1000.0 * concurrency as f64) / ops_delta as f64
                    } else {
                        time_delta * 1000.0 / ops_delta as f64
                    };
                    
                    pb.set_message(format!(
                        "{:.0} ops/s | {:.1} MiB/s | avg {:.2}ms",
                        ops_per_sec, mib_per_sec, avg_latency_ms
                    ));
                }
                
                last_ops = current_ops;
                last_bytes = current_bytes;
                last_time = Instant::now();
            }
        }
    })
}

impl WorkloadRunner {
    pub fn new(config: DlioConfig) -> Self {
        // Load environment variables for S3 credentials
        if let Err(e) = dotenvy::dotenv() {
            warn!("Could not load .env file: {}", e);
        }

        Self {
            config: Arc::new(config),
            metrics: Arc::new(Metrics::new()),
            accelerators: 1, // Default to 1 accelerator
            strict_au: false, // Default to non-strict mode
            rank: 0, // Default to single-process mode
            world_size: 1, // Default to single-process mode
            file_list: None,
            plugins: None, // Plugins are optional, passed via with_plugins()
            checkpoint_state: None,
            multi_endpoint_store: None,
            live_ops: Arc::new(AtomicU64::new(0)),
            live_bytes: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Set plugin manager for checkpoint and other plugin functionality
    pub fn with_plugins(mut self, plugins: PluginManager) -> Self {
        self.plugins = Some(plugins);
        self
    }
    
    /// Set checkpoint state for resuming from a previous run
    pub fn with_checkpoint(mut self, checkpoint_state: crate::plugins::checkpoint::CheckpointState) -> Self {
        info!("🔄 WorkloadRunner configured to resume from step {} (epoch {:?})", 
              checkpoint_state.step, checkpoint_state.epoch);
        self.checkpoint_state = Some(checkpoint_state);
        self
    }

    /// Set accelerator configuration for AU calculation
    pub fn with_accelerator_config(mut self, accelerators: u32, strict_au: bool) -> Self {
        self.accelerators = accelerators;
        self.strict_au = strict_au;
        self
    }

    /// Set multi-rank configuration for distributed execution
    pub fn with_rank_config(mut self, rank: u32, world_size: u32, file_list: Option<Vec<String>>) -> Self {
        self.rank = rank;
        self.world_size = world_size;
        self.file_list = file_list;
        self
    }

    /// Execute ONLY the training phase for DLIO compliance measurement
    /// Data generation should be done separately and is NOT measured
    pub async fn run_training_phase(&mut self) -> Result<()> {
        info!(
            "Starting DLIO training phase measurement: {:?}",
            self.config.model
        );

        // Only measure the training phase - data generation is separate
        let training_start = Instant::now();
        
        println!("📊 Phase: Training (MEASURED for AU calculation)");
        self.run_training().await?;
        
        let training_time = training_start.elapsed();
        info!("Training phase completed in {:?}", training_time);

        // Record training time (NOT total time) for AU calculation
        self.metrics.set_total_time(training_time);
        self.metrics.print_summary();
        
        // v0.8.5: Print per-endpoint statistics if using multi-endpoint store
        if let Some(ref store) = self.multi_endpoint_store {
            let endpoint_stats = store.get_all_stats();
            if !endpoint_stats.is_empty() {
                println!("\n╔═══════════════════════════════════════════════════════════════════════╗");
                println!("║              MULTI-ENDPOINT PERFORMANCE STATISTICS                    ║");
                println!("╚═══════════════════════════════════════════════════════════════════════╝");
                println!();
                for (i, (uri, stats)) in endpoint_stats.iter().enumerate() {
                    println!("Endpoint [{}]: {}", i + 1, uri);
                    println!("  Requests:      {}", stats.total_requests);
                    println!("  Bytes Read:    {} ({:.2} MB)", stats.bytes_read, stats.bytes_read as f64 / (1024.0 * 1024.0));
                    println!("  Bytes Written: {} ({:.2} MB)", stats.bytes_written, stats.bytes_written as f64 / (1024.0 * 1024.0));
                    println!("  Errors:        {}", stats.error_count);
                    println!("  Active Conns:  {}", stats.active_requests);
                    println!();
                }
            }
        }
        
        // Calculate Accelerator Utilization (AU) if metric configuration is present
        debug!("Checking for metric configuration");
        if let Some(metric_config) = &self.config.metric {
            debug!("Metric config found: {:?}", metric_config);
            println!("\n=== Accelerator Utilization (AU) Analysis ===");
            debug!("Train config: {:?}", self.config.train);
            debug!("Calling compute_au with training_time={:?}, accelerators={}", training_time, self.accelerators);
            if let Some(au_result) = (*self.metrics).compute_au(&self.config, training_time, self.accelerators) {
                debug!("compute_au returned result: {:?}", au_result);
                println!("AU Result: {:.1}% ({:.3} fraction)", au_result.au_percent, au_result.au_fraction);
                
                if let Some(pass) = au_result.pass {
                    let threshold = metric_config.au.unwrap_or(0.90);
                    debug!("AU pass/fail evaluation: pass={}, threshold={:.3}", pass, threshold);
                    if pass {
                        println!("✅ AU PASS: {:.1}% >= {:.1}% threshold", au_result.au_percent, threshold * 100.0);
                    } else {
                        println!("❌ AU FAIL: {:.1}% < {:.1}% threshold", au_result.au_percent, threshold * 100.0);
                        
                        // In strict mode, AU failure should cause the workload to fail
                        if self.strict_au {
                            return Err(anyhow::anyhow!(
                                "Strict AU mode: AU {:.1}% is below threshold {:.1}% - storage system is too slow for MLPerf compliance", 
                                au_result.au_percent, threshold * 100.0
                            ));
                        }
                    }
                } else {
                    debug!("AU pass/fail not configured (no threshold in metric config)");
                    println!("AU threshold not configured for pass/fail");
                }
            } else {
                debug!("compute_au returned None - no timing data available");
                println!("AU calculation not available (missing timing data)");
            }
            println!("==============================================");
        }
        
        Ok(())
    }

    /// Execute complete workflow (for backward compatibility)
    /// NOTE: For proper DLIO compliance, use run_training_phase() after separate data generation
    pub async fn run(&mut self) -> Result<()> {
        info!("WARNING: Using legacy run() method - consider using run_training_phase() for proper DLIO compliance");
        
        let start_time = Instant::now();

        // Phase 1: Data Generation (if enabled) - NOT MEASURED
        if self.config.workflow.as_ref().map_or(false, |w| w.generate_data.unwrap_or(false)) {
            info!("Phase 1: Generating data (NOT measured)");
            self.run_data_generation().await?;
        }

        // Phase 2: Training (measured)
        if self.config.workflow.as_ref().map_or(false, |w| w.train.unwrap_or(false)) {
            return self.run_training_phase().await;
        }

        let total_time = start_time.elapsed();
        info!("Workload completed in {:?}", total_time);
        Ok(())
    }

    /// Data generation phase using s3dlio for high-performance storage operations
    /// Supports 3 directory organization modes:
    /// 1. Flat: All files in single directory (Mode 1)
    /// 2. DLIO-style sharding: Files distributed across train/NNNN subdirectories (Mode 2)
    /// 3. Hierarchical: Multi-level nested directory tree (Mode 3)
    pub async fn run_data_generation(&mut self) -> Result<()> {
        use crate::directory_tree::DirectoryMode;
        
        let start_time = Instant::now();
        info!("Starting data generation phase");

        // Determine directory organization mode from config
        let dir_mode = DirectoryMode::from_config(
            self.config.dataset.directory_tree.as_ref(),
            self.config.dataset.num_subfolders_train,
        )?;
        
        match &dir_mode {
            DirectoryMode::Flat => {
                info!("Directory mode: Flat (all files in single directory)");
            }
            DirectoryMode::DlioSharding { num_subfolders } => {
                info!("Directory mode: DLIO-style sharding ({} subfolders)", num_subfolders);
            }
            DirectoryMode::Hierarchical { tree } => {
                info!("Directory mode: Hierarchical (width={}, depth={}, {} dirs, {} files)",
                    tree.config().width, tree.config().depth,
                    tree.total_directories(), tree.total_files());
            }
        }

        // Create object store for the configured storage backend
        let store = self.create_object_store()?;
        let data_folder = &self.config.dataset.data_folder;

        // Create directory structure if needed (filesystem only, not object stores)
        let dirs_to_create = dir_mode.get_directories_to_create(data_folder);
        if !dirs_to_create.is_empty() {
            info!("Creating {} directories for {} backend", 
                dirs_to_create.len(),
                if data_folder.starts_with("file://") { "filesystem" } 
                else if data_folder.starts_with("direct://") { "direct I/O" }
                else { "object store (implicit)" }
            );
            
            for dir_path in &dirs_to_create {
                let full_dir_uri = if data_folder.ends_with('/') {
                    format!("{}{}", data_folder, dir_path)
                } else {
                    format!("{}/{}", data_folder, dir_path)
                };
                
                // Use s3dlio's mkdir (added in v0.9.11)
                // Some backends (like direct://) don't support mkdir, so tolerate failures
                match store.mkdir(&full_dir_uri).await {
                    Ok(_) => {
                        debug!("Created directory: {}", full_dir_uri);
                    }
                    Err(e) => {
                        // If it's a "not implemented" error, that's OK for backends like direct://
                        let err_msg = e.to_string();
                        if err_msg.contains("not implemented") || err_msg.contains("Not implemented") {
                            info!("Directory creation not supported for {} backend (expected for direct://)", 
                                if data_folder.starts_with("direct://") { "direct://" } else { "this" });
                        } else {
                            // For other errors, propagate them
                            return Err(e).with_context(|| format!("Failed to create directory: {}", full_dir_uri));
                        }
                    }
                }
            }
            info!("Directory structure setup completed");
        }

        // Determine number of files to generate
        let num_files = match &dir_mode {
            DirectoryMode::Hierarchical { tree } => {
                // Mode 3: Use tree's total file count
                tree.total_files()
            }
            _ => {
                // Mode 1 & 2: Use num_files_train from config
                self.config.dataset.num_files_train.unwrap_or(100)
            }
        };

        let samples_per_file = self.config.dataset.num_samples_per_file.unwrap_or(1);
        let record_size = self.config.dataset.record_length_bytes.unwrap_or(1024);
        let format = self.config.dataset.format.as_deref().unwrap_or("npz");

        info!(
            "Generating {} files with {} samples each ({}B per record)",
            num_files, samples_per_file, record_size
        );

        // v0.8.6: Reset live stats counters
        self.live_ops.store(0, Ordering::Relaxed);
        self.live_bytes.store(0, Ordering::Relaxed);

        // v0.8.6: Create enhanced progress bar with live stats
        use indicatif::{ProgressBar, ProgressStyle};
        let pb = ProgressBar::new(num_files as u64);
        pb.set_style(ProgressStyle::with_template(
            "{spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {pos}/{len} files {msg}"
        )?);
        pb.set_message("starting generation...");

        // v0.8.6: Spawn live stats monitor
        let monitor_handle = spawn_live_stats_monitor(
            pb.clone(),
            self.live_ops.clone(),
            self.live_bytes.clone(),
            1,  // Single-threaded generation
            num_files as u64,
        );

        // Generate data files using directory mode for path resolution
        for file_idx in 0..num_files {
            // Get file path based on directory mode
            let rel_path = dir_mode.get_file_path(file_idx, format);
            let full_path = if data_folder.ends_with('/') {
                format!("{}{}", data_folder, rel_path)
            } else {
                format!("{}/{}", data_folder, rel_path)
            };

            let data = self.generate_file_data(samples_per_file, record_size)?;

            let write_start = Instant::now();
            store
                .put(&full_path, &data)
                .await
                .with_context(|| format!("Failed to write file {}", full_path))?;
            let write_time = write_start.elapsed();

            // Record metrics (with histogram collection for v0.8.1)
            let bytes_written = (samples_per_file as u64) * (record_size as u64);
            self.metrics
                .record_write_operation(bytes_written, write_time);
            
            // v0.8.1: Record histogram data for accurate write percentiles
            self.metrics.record_write_with_histogram(bytes_written as usize, write_time);
            
            // v0.8.6: Update live counters AFTER successful write
            self.live_ops.fetch_add(1, Ordering::Relaxed);
            self.live_bytes.fetch_add(bytes_written, Ordering::Relaxed);
            
            // v0.8.6: Update progress bar position
            pb.inc(1);
        }

        // v0.8.6: Wait for monitor to complete
        monitor_handle.await.ok();
        
        // v0.8.6: Finish progress bar with summary
        let total_bytes = (num_files as u64) * (record_size as u64) * (samples_per_file as u64);
        pb.finish_with_message(format!(
            "generated {} files ({:.2} GiB total)",
            num_files,
            total_bytes as f64 / 1_073_741_824.0
        ));

        let generation_time = start_time.elapsed();
        
        // v0.8.6: Display performance summary with latency percentiles (like sai3-bench)
        let total_ops = self.live_ops.load(Ordering::Relaxed);
        let total_bytes_atomic = self.live_bytes.load(Ordering::Relaxed);
        let ops_per_sec = total_ops as f64 / generation_time.as_secs_f64();
        let throughput_mibs = (total_bytes_atomic as f64 / 1_048_576.0) / generation_time.as_secs_f64();
        
        // Get latency percentiles from write histograms
        let write_hists = self.metrics.get_write_histograms();
        let combined = write_hists.combined_histogram();
        
        if combined.len() > 0 {
            let mean_ms = combined.mean() / 1_000.0;
            let p50_ms = combined.value_at_quantile(0.50) as f64 / 1_000.0;
            let p90_ms = combined.value_at_quantile(0.90) as f64 / 1_000.0;
            let p95_ms = combined.value_at_quantile(0.95) as f64 / 1_000.0;
            let p99_ms = combined.value_at_quantile(0.99) as f64 / 1_000.0;
            
            println!("✅ Generated {} files ({:.2} GiB) in {:.2}s @ {:.1} MiB/s", 
                num_files, 
                total_bytes as f64 / 1_073_741_824.0,
                generation_time.as_secs_f64(),
                throughput_mibs
            );
            println!("   Latency: mean={:.2}ms, p50={:.2}ms, p90={:.2}ms, p95={:.2}ms, p99={:.2}ms",
                mean_ms, p50_ms, p90_ms, p95_ms, p99_ms);
        } else {
            println!("✅ Generated {} files ({:.2} GiB) in {:.2}s @ {:.1} MiB/s", 
                num_files, 
                total_bytes as f64 / 1_073_741_824.0,
                generation_time.as_secs_f64(),
                throughput_mibs
            );
        }
        
        info!("Data generation completed in {:?}", generation_time);
        info!("Generated {} files in {:?} ({:.2} MiB/s)", 
            num_files, 
            generation_time,
            throughput_mibs
        );
        Ok(())
    }

    /// Training phase using DLIO-style parallel I/O with background workers
    /// TRUE DLIO PARALLEL I/O MODEL - Background workers + instant batch retrieval
    /// 
    /// v0.8.6: Uses per-epoch progress bars with live performance statistics
    /// Each epoch shows its own progress bar with real-time ops/s, MiB/s, and avg latency
    async fn run_training(&mut self) -> Result<()> {
        use indicatif::{ProgressBar, ProgressStyle};
        
        let epochs = self.config.train.as_ref().and_then(|t| t.epochs).unwrap_or(1);
        let batch_size = self.config.reader.batch_size.unwrap_or(16);
        let read_threads = self.config.reader.read_threads.unwrap_or(8) as usize;
        let prefetch_size = self.config.reader.prefetch.unwrap_or(4);

        info!("🚀 TRUE DLIO PARALLEL MODEL: {} epochs, batch_size={}, read_threads={}, prefetch_queue={}", 
              epochs, batch_size, read_threads, prefetch_size);

        // Create s3dlio dataset with the SAME store used for generation
        // This ensures multi-endpoint configuration is respected during training
        let data_folder = self.config.dataset.data_folder.clone();
        let store = self.create_object_store()?;  // Reuse our multi-endpoint store
        let dataset = self.create_multi_backend_dataset_with_store(&data_folder, store).await?;
        let total_files = dataset.len();
        
        let estimated_batches_per_epoch = (total_files + batch_size - 1) / batch_size;
        info!("📂 Dataset: {} files, ~{} batches per epoch", total_files, estimated_batches_per_epoch);
        debug!("Dataset configuration: total_files={}, batch_size={}, estimated_batches={}", 
               total_files, batch_size, estimated_batches_per_epoch);
        trace!("Full dataset path: {}", data_folder);

        // Determine starting epoch from checkpoint if resuming
        // Note: We resume at the START of the next epoch after the checkpoint
        // This avoids complexity of mid-epoch resumption and matches common ML framework behavior
        let start_epoch = if let Some(ref checkpoint) = self.checkpoint_state {
            let checkpoint_epoch = checkpoint.epoch.unwrap_or(0) as usize;
            // Resume at the next epoch after the checkpoint
            let resume_epoch = checkpoint_epoch + 1;
            info!("🔄 Resuming from checkpoint: completed epoch {}, resuming at epoch {} (step {})", 
                  checkpoint_epoch, resume_epoch, checkpoint.step);
            
            // Restore checkpoint state in plugins
            if let Some(ref mut plugins) = self.plugins {
                if let Some(checkpoint_plugin) = plugins.get_checkpoint_plugin_mut() {
                    checkpoint_plugin.restore_from_checkpoint(checkpoint);
                    info!("✅ CheckpointPlugin state restored");
                } else {
                    warn!("⚠️  CheckpointPlugin not found - checkpoint state not restored in plugin");
                }
            }
            
            resume_epoch
        } else {
            0
        };

        for epoch in start_epoch..epochs as usize {
            let epoch_start = Instant::now();
            println!("🏃 Epoch {}/{} starting...", epoch + 1, epochs);
            info!("Epoch {}/{} - Starting TRUE parallel I/O + compute", epoch + 1, epochs);
            debug!("Epoch {} configuration: read_threads={}, prefetch_size={}", epoch + 1, read_threads, prefetch_size);
            trace!("Epoch {} detailed timing started at {:?}", epoch + 1, epoch_start);

            // v0.8.6: Reset live stats counters for each epoch
            self.live_ops.store(0, Ordering::Relaxed);
            self.live_bytes.store(0, Ordering::Relaxed);

            // v0.8.6: Create per-epoch progress bar with live stats
            let progress = ProgressBar::new(estimated_batches_per_epoch as u64);
            progress.set_style(
                ProgressStyle::with_template(
                    "{spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {pos}/{len} batches {msg}"
                ).expect("Failed to set progress bar template")
            );
            progress.set_message("starting epoch...");

            // v0.8.6: Spawn live stats monitor for this epoch
            let monitor_handle = spawn_live_stats_monitor(
                progress.clone(),
                self.live_ops.clone(),
                self.live_bytes.clone(),
                read_threads,
                estimated_batches_per_epoch as u64,
            );

            let mut batch_count = 0;
            let mut total_samples = 0;
            let mut total_bytes = 0;
            let mut total_io_time = Duration::ZERO;
            let mut total_compute_time = Duration::ZERO;

            // === CRITICAL: TRUE DLIO PARALLEL MODEL ===
            // Background I/O workers continuously load batches into channel
            // Main thread gets batches instantly while background loads next batches
            let (batch_tx, mut batch_rx) = tokio::sync::mpsc::channel::<Result<Vec<Vec<u8>>>>(prefetch_size * 2);
            
            // Configure aggressive s3dlio loading
            let pool_config = PoolConfig {
                pool_size: read_threads,
                readahead_batches: prefetch_size * 2, // Aggressive prefetching
                batch_timeout: Duration::from_secs(30),
                max_inflight: read_threads * 4, // Very high concurrency
            };

            let loader_options = LoaderOptions {
                batch_size: batch_size,
                prefetch: prefetch_size,
                shuffle: false, // Consistent ordering for debugging
                num_workers: read_threads,
                reader_mode: ReaderMode::Sequential,
                loading_mode: LoadingMode::AsyncPool(pool_config.clone()),
                ..Default::default()
            };

            // === BACKGROUND I/O WORKER TASK ===
            let dataset_clone = dataset.clone();
            let background_io = tokio::spawn(async move {
                info!("🔄 Background I/O workers starting with {} threads, {} prefetch", read_threads, prefetch_size);
                
                let async_loader = AsyncPoolDataLoader::new(dataset_clone, loader_options);
                let mut stream = async_loader.stream_with_pool(pool_config);
                
                let mut bg_batch_count = 0;
                while let Some(batch_result) = stream.next().await {
                    bg_batch_count += 1;
                    
                    if batch_tx.send(batch_result.map_err(anyhow::Error::from)).await.is_err() {
                        debug!("Main thread finished, stopping background I/O at batch {}", bg_batch_count);
                        break;
                    }
                    
                    if bg_batch_count % 10 == 0 {
                        debug!("Background I/O: loaded {} batches, queue filling continuously...", bg_batch_count);
                    }
                }
                info!("🛑 Background I/O completed: {} batches loaded", bg_batch_count);
            });

            info!("⚡ PARALLEL MODE ACTIVE: Background loading batches, main thread consuming with compute overlap");

            // === MAIN COMPUTE THREAD ===
            // This should get batches INSTANTLY from prefetch queue
            while let Some(batch_result) = batch_rx.recv().await {
                match batch_result {
                    Ok(batch) => {
                        let batch_start = Instant::now();
                        
                        // === I/O TIME MEASUREMENT ===
                        // With proper background I/O, this should be microseconds
                        let io_start = Instant::now();
                        let batch_size_actual = batch.len();
                        let batch_bytes: usize = batch.iter().map(|item| item.len()).sum();
                        
                        // Minimal validation (represents data preprocessing)
                        let _checksum: u64 = batch.iter().take(1)
                            .map(|item| item.iter().take(10).map(|&b| b as u64).sum::<u64>())
                            .sum();
                        let io_time = io_start.elapsed(); // Should be ~microseconds!
                        
                        // === COMPUTE TIME ===
                        // While we compute, background workers load next batches = TRUE PARALLELISM
                        let compute_start = Instant::now();
                        self.process_batch(&batch).await?;
                        let compute_time = compute_start.elapsed();
                        
                        let batch_total_time = batch_start.elapsed();

                        // Accumulate for AU calculation
                        total_io_time += io_time;
                        total_compute_time += compute_time;
                        
                        // Record metrics (with histogram collection for v0.8.1)
                        self.metrics.record_bytes_read(batch_bytes as u64);
                        self.metrics.record_read_time(io_time);
                        self.metrics.record_compute_time(compute_time);
                        self.metrics.record_batch_time(batch_total_time);
                        
                        // v0.8.1: Record histogram data for accurate percentiles
                        self.metrics.record_read_with_histogram(batch_bytes, io_time);

                        // v0.8.6: Update live counters after successful batch processing
                        self.live_ops.fetch_add(1, Ordering::Relaxed);
                        self.live_bytes.fetch_add(batch_bytes as u64, Ordering::Relaxed);

                        batch_count += 1;
                        total_samples += batch_size_actual;
                        total_bytes += batch_bytes;

                        // Call plugin hook for step-based checkpointing
                        let global_step = (epoch as usize * estimated_batches_per_epoch) + batch_count;
                        if let Some(ref mut plugins) = self.plugins {
                            plugins.after_step(global_step as u32).await?;
                        }

                        // v0.8.6: Update per-epoch progress bar
                        progress.inc(1);

                        // Show parallel processing effectiveness (less frequently with progress bar)
                        if batch_count % 5 == 0 || batch_count < 5 {
                            let io_ms = io_time.as_secs_f64() * 1000.0;
                            let compute_ms = compute_time.as_secs_f64() * 1000.0;
                            info!(
                                "PARALLEL Batch {} | {} files, {:.1}MB | I/O: {:.2}ms, Compute: {:.1}ms | Background: loading next...",
                                batch_count, batch_size_actual, batch_bytes as f64 / 1_000_000.0, io_ms, compute_ms
                            );
                        }
                    }
                    Err(e) => {
                        // v0.8.6: Stop monitor and finish progress bar on error
                        monitor_handle.abort();
                        progress.finish_with_message("❌ Failed");
                        
                        error!("Background I/O error: {}", e);
                        return Err(e.into());
                    }
                }
            }

            // Wait for background task
            if let Err(e) = background_io.await {
                warn!("Background I/O task error: {:?}", e);
            }
            
            // v0.8.6: Wait for monitor to complete and finish progress bar
            monitor_handle.await.ok();
            progress.finish();
            
            // === EPOCH ANALYSIS ===
            let epoch_total_time = epoch_start.elapsed();
            self.metrics.record_epoch_time(epoch_total_time);
            
            let au_percentage = if epoch_total_time.as_secs_f64() > 0.0 {
                (total_compute_time.as_secs_f64() / epoch_total_time.as_secs_f64()) * 100.0
            } else {
                0.0
            };

            // v0.8.6: Get batch latency percentiles for this epoch
            let batch_hists = self.metrics.get_batch_histograms();
            let (has_samples, mean_ms, p50_ms, p90_ms, p95_ms, p99_ms) = {
                let batch_hist_locked = batch_hists.hist.lock().unwrap();
                if batch_hist_locked.len() > 0 {
                    (
                        true,
                        batch_hist_locked.mean() / 1_000.0,
                        batch_hist_locked.value_at_quantile(0.50) as f64 / 1_000.0,
                        batch_hist_locked.value_at_quantile(0.90) as f64 / 1_000.0,
                        batch_hist_locked.value_at_quantile(0.95) as f64 / 1_000.0,
                        batch_hist_locked.value_at_quantile(0.99) as f64 / 1_000.0,
                    )
                } else {
                    (false, 0.0, 0.0, 0.0, 0.0, 0.0)
                }
            };
            
            // User-facing epoch summary with latency percentiles
            if has_samples && batch_count > 0 {
                let throughput_mibs = (total_bytes as f64 / 1_048_576.0) / epoch_total_time.as_secs_f64();
                
                println!(
                    "✅ Epoch {}/{} complete: {} batches, {} samples, {:.1}MiB in {:.2}s @ {:.1} MiB/s",
                    epoch + 1, epochs, batch_count, total_samples, 
                    total_bytes as f64 / 1_048_576.0, epoch_total_time.as_secs_f64(), throughput_mibs
                );
                println!("   Latency: mean={:.2}ms, p50={:.2}ms, p90={:.2}ms, p95={:.2}ms, p99={:.2}ms",
                    mean_ms, p50_ms, p90_ms, p95_ms, p99_ms);
            } else {
                let throughput_mibs = (total_bytes as f64 / 1_048_576.0) / epoch_total_time.as_secs_f64();
                println!(
                    "✅ Epoch {}/{} complete: {} batches, {} samples, {:.1}MiB in {:.2}s @ {:.1} MiB/s",
                    epoch + 1, epochs, batch_count, total_samples, 
                    total_bytes as f64 / 1_048_576.0, epoch_total_time.as_secs_f64(), throughput_mibs
                );
            }
            
            info!(
                "Epoch {} COMPLETE | {} batches, {} samples, {:.1}MiB in {:?}",
                epoch + 1, batch_count, total_samples, total_bytes as f64 / 1_048_576.0, epoch_total_time
            );
            
            if batch_count > 0 {
                let avg_io_ms = (total_io_time.as_secs_f64() / batch_count as f64) * 1000.0;
                let avg_compute_ms = (total_compute_time.as_secs_f64() / batch_count as f64) * 1000.0;
                
                info!(
                    "📊 TIMING | Avg I/O: {:.2}ms, Avg Compute: {:.1}ms, AU: {:.1}%", 
                    avg_io_ms, avg_compute_ms, au_percentage
                );

                // Validate parallel effectiveness
                if avg_io_ms < 10.0 && au_percentage < 80.0 {
                    info!("🎉 PARALLEL SUCCESS: I/O {:.1}ms (near-instant!), AU {:.1}% (realistic parallel)", 
                          avg_io_ms, au_percentage);
                } else if avg_io_ms > 50.0 {
                    warn!("⚠️  SEQUENTIAL DETECTED: I/O {:.1}ms (too slow), indicates poor parallelism", avg_io_ms);
                } else if au_percentage > 90.0 {
                    warn!("⚠️  HIGH AU: {:.1}% suggests sequential processing, not parallel I/O", au_percentage);
                }
            }

            // Call plugin hook for epoch-based checkpointing
            if let Some(ref mut plugins) = self.plugins {
                plugins.after_epoch(epoch as u32).await?;
            }
        }

        // Finalize plugins (cleanup, final checkpoint, etc.)
        if let Some(ref mut plugins) = self.plugins {
            plugins.finalize().await?;
        }

        info!("🏁 DLIO parallel training completed");
        Ok(())
    }

    /// Checkpointing phase (placeholder for future implementation)
    #[allow(dead_code)]
    async fn run_checkpointing(&mut self) -> Result<()> {
        info!("Checkpointing phase - placeholder");
        // TODO: Implement checkpointing using s3dlio's checkpoint module
        Ok(())
    }

    /// Create object store instance based on storage backend configuration
    /// Supports multi-endpoint load balancing when multiple endpoint_uris are configured
    /// 
    /// Returns Arc<dyn ObjectStore> for shared ownership across async tasks.
    /// For multi-endpoint stores, also saves typed Arc<MultiEndpointStore> for stats access.
    fn create_object_store(&mut self) -> Result<Arc<dyn ObjectStore>> {
        let data_folder = &self.config.dataset.data_folder;
        
        // Check if multi-endpoint configuration is present
        if let Some(endpoint_uris) = &self.config.dataset.endpoint_uris {
            if endpoint_uris.len() > 1 {
                info!("Creating multi-endpoint store with {} endpoints using {} strategy",
                      endpoint_uris.len(), self.config.dataset.load_balance_strategy);
                
                // Parse strategy
                let strategy = match self.config.dataset.load_balance_strategy.as_str() {
                    "least_connections" => LoadBalanceStrategy::LeastConnections,
                    _ => LoadBalanceStrategy::RoundRobin,
                };
                
                // Create multi-endpoint store using s3dlio v0.9.16+ API
                let store = Arc::new(MultiEndpointStore::new(
                    endpoint_uris.clone(),
                    strategy,
                    None, // use default thread count
                )?);
                
                // Keep typed reference for stats access (same allocation, two views)
                self.multi_endpoint_store = Some(store.clone());
                
                // Return trait object view (unsize coercion: Arc<T> -> Arc<dyn Trait>)
                return Ok(store as Arc<dyn ObjectStore>);
            }
        }
        
        // Single endpoint (default behavior)
        info!("Creating object store for: {}", data_folder);
        let store = store_for_uri(data_folder)
            .with_context(|| format!("Failed to create object store for {}", data_folder))?;
        // Box<dyn ObjectStore> -> Arc<dyn ObjectStore> via Arc::from
        Ok(Arc::from(store))
    }

    /// Generate data for a single file
    fn generate_file_data(&self, samples: usize, record_size: usize) -> Result<Vec<u8>> {
        // Generate synthetic data based on format
        match self.config.dataset.format.as_deref().unwrap_or("npz") {
            "npz" => {
                // Use s3dlio's data generation utilities
                // Note: generate_controlled_data takes (size, dedup, compress)
                let total_size = samples * record_size;
                let data = s3dlio::generate_controlled_data(total_size, 0, 0);
                Ok(data)
            }
            _ => {
                // Generate random data for other formats
                let total_size = samples * record_size;
                let data = (0..total_size).map(|i| (i % 256) as u8).collect();
                Ok(data)
            }
        }
    }

    pub fn get_metrics(&self) -> &Metrics {
        &self.metrics
    }

    /// Apply realistic framework-specific workload profile (Workstream A)
    pub fn with_profile(self, profile_name: &str) -> Result<Self> {
        info!("Applying {} workload profile", profile_name);
        
        // Apply the profile based on name - this configures LoaderOptions/PoolConfig
        // The actual s3dlio configuration will be applied when creating the data loader
        match profile_name {
            "torch-like" | "torch" | "pytorch" => {
                // torch_like() returns (LoaderOptions, PoolConfig) but we don't store them here
                // We'll apply them in the data loading phase
                info!("PyTorch-like profile: Large batches, moderate parallelism, memory-efficient");
            }
            "tf-like" | "tf" | "tensorflow" => {
                info!("TensorFlow-like profile: Medium batches, high parallelism, streaming-optimized");
            }
            "jax-like" | "jax" => {
                info!("JAX-like profile: Variable batches, maximum parallelism, throughput-optimized");
            }
            _ => {
                return Err(anyhow::anyhow!("Unknown profile: {}. Available: torch-like, tf-like, jax-like", profile_name));
            }
        }
        
        // Store the profile name for later use during data loading
        // For now, we'll add it as a field in a future update
        // TODO: Add profile field to WorkloadRunner struct
        
        Ok(self)
    }

    /// Create MultiBackendDataset for unified access across all storage backends
    /// Create dataset using provided store (supports multi-endpoint configuration)
    /// This is the CORRECT method - it reuses the store we created with multi-endpoint support
    async fn create_multi_backend_dataset_with_store(
        &self, 
        data_folder: &str, 
        store: Arc<dyn ObjectStore>
    ) -> Result<MultiBackendDataset> {
        info!("Creating MultiBackendDataset for folder: {}", data_folder);

        // List URIs using our multi-endpoint store
        let uris = store.list(data_folder, true).await
            .with_context(|| format!("Failed to list files from: {}", data_folder))?;

        info!("Successfully created dataset with {} files", uris.len());
        
        // Construct dataset directly with our store (preserves multi-endpoint config)
        Ok(MultiBackendDataset { uris, store })
    }
    
    /// DEPRECATED: Creates its own single-endpoint store, ignoring multi-endpoint config
    /// Kept for reference but should not be used
    #[allow(dead_code)]
    async fn create_multi_backend_dataset(&self, data_folder: &str) -> Result<MultiBackendDataset> {
        info!("Creating MultiBackendDataset for folder: {}", data_folder);

        // WARNING: This calls from_prefix() which creates a NEW single-endpoint store
        // Multi-endpoint configuration is IGNORED!
        let dataset = MultiBackendDataset::from_prefix(data_folder)
            .await
            .with_context(|| format!("Failed to create dataset from prefix: {}", data_folder))?;

        info!("Successfully created dataset with {} files", dataset.len());
        Ok(dataset)
    }

    /// Process a batch of data (simulate training computation with exact DLIO timing)
    async fn process_batch(&self, _batch: &[Vec<u8>]) -> Result<()> {
        // Use exact computation_time from DLIO config (per step, not per sample)
        if let Some(computation_time) = self.config.train.as_ref().and_then(|t| t.computation_time) {
            if computation_time > 0.0 {
                let processing_delay = std::time::Duration::from_secs_f64(computation_time);
                tokio::time::sleep(processing_delay).await;
            }
        }
        // If no computation_time specified, no artificial delay (matches DLIO behavior)
        Ok(())
    }
    
    /// Export multi-endpoint statistics to TSV file (if multi-endpoint store was used)
    pub fn export_endpoint_stats<P: AsRef<std::path::Path>>(&self, output_path: P, wall_seconds: f64) -> Result<()> {
        if let Some(ref store) = self.multi_endpoint_store {
            let endpoint_stats = store.get_all_stats();
            crate::tsv_export::export_endpoint_stats(output_path, &endpoint_stats, wall_seconds)?;
            info!("Multi-endpoint statistics exported");
        }
        Ok(())
    }
    
    /// Print multi-endpoint statistics to console (if multi-endpoint store was used)
    pub fn print_endpoint_stats(&self) {
        if let Some(ref store) = self.multi_endpoint_store {
            let endpoint_stats = store.get_all_stats();
            if !endpoint_stats.is_empty() {
                println!("\n┌─ Multi-Endpoint Statistics ──────────────────────────────────────────┐");
                println!("│ Endpoint Performance Summary:");
                for (i, (uri, stats)) in endpoint_stats.iter().enumerate() {
                    println!("│");
                    println!("│ Endpoint [{}]: {}", i + 1, uri);
                    println!("│   Requests:      {}", stats.total_requests);
                    println!("│   Bytes Read:    {} ({:.2} MB)", stats.bytes_read, stats.bytes_read as f64 / (1024.0 * 1024.0));
                    println!("│   Bytes Written: {} ({:.2} MB)", stats.bytes_written, stats.bytes_written as f64 / (1024.0 * 1024.0));
                    println!("│   Errors:        {}", stats.error_count);
                    println!("│   Active Conns:  {}", stats.active_requests);
                }
                println!("└──────────────────────────────────────────────────────────────────────┘");
            }
        }
    }
}

