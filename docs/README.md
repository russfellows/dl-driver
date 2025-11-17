# dl-driver Documentation

**Version:** 0.8.9  
**Last Updated:** October 12, 2025

## User Documentation

### Essential Guides

- **[USER_GUIDE.md](USER_GUIDE.md)** - Comprehensive user guide covering all execution modes, configuration, storage backends, and troubleshooting
- **[QUICK_START.md](QUICK_START.md)** - Get started quickly with basic examples
- **[Changelog.md](Changelog.md)** - Complete version history and release notes

### Technical Reference

- **[DUAL_METRICS_REPORTING.md](DUAL_METRICS_REPORTING.md)** - Detailed specification of the dual metrics system (storage + AI/ML perspectives)
- **[LICENSE-COMPLIANCE.md](LICENSE-COMPLIANCE.md)** - License compliance information and third-party dependencies

### Distributed Execution

- **[tests/dlio_configs/DISTRIBUTED_README.md](../tests/dlio_configs/DISTRIBUTED_README.md)** - Complete guide for multi-agent distributed workloads

## Additional Resources

### Subdirectories

- **[releases/](releases/)** - Release notes and pull request summaries
- **[testing/](testing/)** - Test results and validation reports
- **[goldens/](goldens/)** - Golden reference files for validation
- **[archive/](archive/)** - Historical planning documents and implementation notes

### Quick Navigation

#### Getting Started
1. Start with [QUICK_START.md](QUICK_START.md) for basic usage
2. Read [USER_GUIDE.md](USER_GUIDE.md) for comprehensive documentation
3. Check [Changelog.md](Changelog.md) for latest features

#### Distributed Execution
1. Read distributed section in [USER_GUIDE.md](USER_GUIDE.md#3-distributed-multi-agent-execution)
2. Follow [DISTRIBUTED_README.md](../tests/dlio_configs/DISTRIBUTED_README.md) for setup
3. Use example configs in `tests/dlio_configs/distributed_*.yaml`

#### Metrics and Analysis
1. Understand dual metrics in [DUAL_METRICS_REPORTING.md](DUAL_METRICS_REPORTING.md)
2. Export metrics to TSV/JSON/CSV for analysis
3. View [USER_GUIDE.md#metrics-and-reporting](USER_GUIDE.md#metrics-and-reporting)

## Documentation Structure

```
docs/
├── README.md                          # This file - documentation index
├── USER_GUIDE.md                      # 📘 Comprehensive user guide (START HERE)
├── QUICK_START.md                     # ⚡ Quick start for impatient users
├── Changelog.md                       # 📝 Version history and release notes
├── DUAL_METRICS_REPORTING.md          # 📊 Metrics specification
├── LICENSE-COMPLIANCE.md              # ⚖️ License information
├── releases/                          # Release documentation
│   ├── v0.4.0-release-notes.md
│   ├── v0.6.0-unified-architecture.md
│   ├── v0.7.0-release-notes.md
│   ├── v0.7.1-release-notes.md
│   └── PULL_REQUEST_SUMMARY.md
├── testing/                           # Test results
│   ├── TEST_RESULTS.md
│   ├── M4_INTEGRATION_TEST_RESULTS.md
│   └── ALL_BACKENDS_TEST_RESULTS.md
├── goldens/                           # Golden reference files
│   ├── README.md
│   ├── tolerance.json
│   └── test_configs/
└── archive/                           # Historical documents
    └── planning/                      # Implementation planning documents
        ├── PHASE1_COMPLETE_SUMMARY.md
        ├── PHASE2_AGENT_IMPLEMENTATION.md
        ├── PHASE3_CONTROLLER_IMPLEMENTATION.md
        ├── PHASE3_TESTING_SUMMARY.md
        └── ... (15+ planning/handoff docs)
```

## Contributing

See the main [README.md](../README.md) for contribution guidelines.

## Support

- **Issues**: https://github.com/russfellows/dl-driver/issues
- **Documentation**: https://github.com/russfellows/dl-driver/tree/main/docs
- **License**: GPL v3.0
