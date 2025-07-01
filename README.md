# real_dlio
Realistic Driver for AI/ML Storage Workloads

# Project to Create Rust based DLIO Tool
## Request
Please analyze the following github repository:  https://github.com/argonne-lcf/dlio_benchmark/tree/main

I want to create a new tool, possibly using the Rust language with Python PyO3 bindings to enable Python interactions.  This new tool would perform similar functions as the DLIO repository, but in a simpler manner.  There is way too much class overloading, etc. in my opinion.  

One key piece of DLIO are the configuration files, located here: https://github.com/argonne-lcf/dlio_benchmark/tree/main/dlio_benchmark/configs/workload

These configurations drive different type of workloads.  I would like the new tools to either use the same config files, or possibly new updated config files that would be used to run different workloads, just as DLIO does.  

Another important piece of information is that I want the new tool to be able to interact with both file and object storage data, via S3.  As an example of an S3 interface that I would like to use is the following Github repo that I own:  https://github.com/russfellows/s3dlio

### Request Action summary:
1 ) Analyze the DLIO tool
2) Create a plan and an outline for a new code project that can recreate key pieces of DLIO in a simplified manner.  I want the new project to perform ANY I/O intensive operations using a compiled, highly performant language such as Rust.  Some pieces of code that are not performance critical can be written in a language such as Python where it would be more productive.  
3) Insure the new tool can use workload configuration files similar to, or ideally the same as those used by DLIO
4) The new tool should also support I/O via both file interface and S3, using the s3dlio library as a guide.

# ChatGPT said:
Request for o3 pro
Reasoned for 3m 11s
Below is a concise technical “gap analysis” of DLIO 2.x followed by a concrete, Rust‑centric project outline that re‑implements the essential pieces with far less indirection, yet remains drop‑in compatible with DLIO workload YAML and adds first‑class S3 support through s3dlio.

## 1  What DLIO actually does (and where the complexity comes from)

Layer	Key responsibilities	Observed problems / bloat
Benchmark Runner	Drives the three high‑level phases (data‑generation → training‑I/O emulation → post‑processing) and orchestrates MPI / multi‑process execution.	Deep inheritance tree, heavy use of Hydra for every parameter, making the control‑flow hard to follow.
github.com
Data Generator	Creates synthetic datasets for NPZ, TFRecord, HDF5, CSV, JPEG, etc.	Generates data in Python with NumPy loops; scaling to TB sizes is CPU‑bound.
Format Handlers (Readers/Writers)	Invoke framework loaders (PyTorch/TF) or custom Python readers.	A class per {framework × format}, most of which just forward to open()/read(); leads to ▹70+ subclasses.
I/O Profiler / Post‑processor	Optional DFTracer hooks plus log parsing via Pandas.	Profiling code is entangled with the runners.
Configuration System	YAML files (one per workload) loaded through Hydra, supporting CLI overrides. Example shown in README.
github.com
Good idea, but Hydra’s deep nested overrides cause a large implicit state‑space; many default values are duplicated.
Storage back‑end	POSIX only; S3 is “future work”.	No abstraction layer—paths are assumed to be local.
## 2  Design goals for the new project (“io‑bench‑lite”)

Keep what matters: the workload descriptions and the measured metrics.
Eliminate Python hot‑loops: all heavy I/O should be in safe, native Rust.
Expose a clean Python API (PyO3) and a single‑file CLI so scripts that already call dlio_benchmark … only need trivial changes.
Pluggable storage: POSIX and S3 (async & sync) via the existing s3dlio crate, whose five primitives (PUT/GET/LIST/DELETE/CREATE) already map one‑to‑one to our needs. 
github.com
Config compatibility: accept unchanged DLIO YAML, but lint it and transform it into a small Rust Config struct (Serde) at runtime; no Hydra required.
Zero‑copy data buffers and systematic re‑use of pre‑allocated Vec<u8> slices (64 B delta per 4 KiB block, mirroring your prior memory‑efficiency spec).

## 3  Proposed code‑base layout
```
io-bench-lite/
├─ Cargo.toml            # workspace
├─ crates/
│  ├─ core/              # generic engine
│  │   ├─ config.rs
│  │   ├─ generator.rs
│  │   ├─ runner.rs
│  │   └─ metrics.rs
│  ├─ storage/
│  │   ├─ mod.rs         # defines StorageBackend trait
│  │   ├─ posix.rs
│  │   └─ s3.rs          # thin wrapper around s3dlio::Client
│  ├─ formats/
│  │   ├─ npz.rs         # uses ndarray-npz
│  │   ├─ hdf5.rs        # uses hdf5‑rust
│  │   └─ tfrs.rs        # TFRecord read/write (prost‑based)
│  └─ py_api/
│      └─ lib.rs         # #[pymodule] bindings (PyO3 + maturin)
└─ cli/
   └─ main.rs            # clap‑based binary (`io-bench`)
```
   
   
Core abstractions
trait StorageBackend {
    async fn put(&self, key: &str, data: &[u8]) -> Result<()>;
    async fn get(&self, key: &str) -> Result<Bytes>;
    async fn list(&self, prefix: &str) -> Result<Vec<String>>;
    async fn delete(&self, prefix: &str) -> Result<()>;
}

trait Format {
    fn generate(&self, cfg: &FormatCfg, buf: &mut [u8]) -> Result<()>;
    fn read(&self, bytes: &[u8]) -> Result<()>;
}
Runner composes StorageBackend + Format and executes phases described in YAML.

## 4  Handling DLIO configuration files

Parse with Serde‑YAML into an untyped Value.
Apply a YAML → Rust Config mapping table:
Keep fields dataset, reader, workflow verbatim.
Ignore framework, train, computation_time, etc.—they only affect GPU kernels in DLIO.
Provide dlio‑to‑lite‑validate CLI to print unknown keys and suggest minimal YAML if you prefer new files.
This approach allows:

io-bench generate --config workloads/unet3d.yaml  # same file, new engine

## 5  S3 integration path

Re‑export the s3dlio client inside storage::s3; users inherit its .env‑based auth (AWS keys, endpoint URL, CA bundle).
Support both blocking (tokio::task::block_in_place) and async modes; select with CLI flag --async.
Multi‑threaded transfers via tokio::task::spawn and a bounded channel; object size stripes map to buffered byte‑ranges to saturate bandwidth.

## 6  Performance & scalability strategy

* Memory – single reusable Vec<u8> per generator thread; mutate 64 B per 4 KiB to avoid page‑fault storm.
* CPU – rayon for on‑node parallelism; optional --mpi feature uses rsmpi to coordinate ranks but keeps the rest of the code identical.
* I/O – leverage O_DIRECT/posix_fadvise on Linux; for S3 use multipart_upload with 8‑32 MiB parts, adaptive to object size.
* Metrics – record nanosecond timestamps around every backend call, accumulate in a Histogram<u64, 1_000_000µs>; expose CSV/JSON.

## 7  Suggested milestone plan

### Milestone	Scope	Target Δ
| Milestone | Description | Time | Status
| :-------- | :---------- | :---- | :------ |
| M0 | Skeleton workspace compiles; CLI echoes parsed YAML. | 1 week | Complete |
| M1 | POSIX backend + NPZ generator/reader + metrics; passes unit tests for ≥10 GB dataset. | 2 weeks | Partial |
| M2 | S3 backend via s3dlio (sync) + multipart upload. | 1 week | TBD |
| M3 | Async S3 path + HDF5 format + basic histogram report. | 2 weeks | TBD |
| M4 | PyO3 bindings (pip install io-bench-lite) + docs + container image. | 1 week | TBD |

## Why this is much simpler
<2000 LoC in Rust vs ~12 k LoC Python + templates in DLIO.
One trait per extensibility axis, no inheritance.
Native concurrency and zero‑copy I/O.
YAML compatibility keeps users’ existing workload files unchanged.
Next steps for you
Confirm which DLIO formats you truly need (NPZ and HDF5 are often enough).
Decide whether MPI support is necessary from day 1 or can be deferred.
Pick a crate name and initialise the workspace—most pieces above can be scaffolded with cargo new --lib.
Feel free to ask for code snippets or deeper dives into any module once you start implementation!


