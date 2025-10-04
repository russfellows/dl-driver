# Migration Quick Start Guide

**For the new agent**: Start here, then read the detailed migration plan.

---

## 📋 Pre-Flight Checklist

- [ ] Read `HANDOFF_SUMMARY.md` (5 min read)
- [ ] Read `MIGRATION_PLAN_S3DLIO_0.8.19.md` (full details)
- [ ] Review current build status: ✅ 61/61 tests passing
- [ ] Understand hybrid approach: Use s3dlio-oplog for parsing, keep dl-driver features

---

## 🚀 Quick Start (30 minutes)

### Step 1: Create Branch (1 min)
```bash
cd /home/eval/Documents/Code/dl-driver
git checkout -b feature/s3dlio-0.8.19-migration
git status  # Ensure clean state
```

### Step 2: Update Root Cargo.toml (2 min)
**File**: `Cargo.toml`

**REMOVE** (lines 11-15):
```toml
[patch.crates-io]
# Use s3dlio's patched version to expose hyper_builder methods for HTTP connection pool configuration
# Note: This still uses path dependency as the patch is in a subdirectory of s3dlio
# TODO: Consider copying the patch locally or publishing it separately
aws-smithy-http-client = { path = "../s3dlio/fork-patches/aws-smithy-http-client" }
```

### Step 3: Update Core Crate (3 min)
**File**: `crates/core/Cargo.toml`

**CHANGE** line 42:
```toml
# Old:
s3dlio = { git = "https://github.com/russfellows/s3dlio.git", rev = "cd4ee2e" }

# New:
s3dlio = { git = "https://github.com/russfellows/s3dlio.git", tag = "v0.8.19" }
```

**ADD** after line 42:
```toml
# s3dlio-oplog for shared operation log parsing/replay
s3dlio-oplog = { path = "../../../s3dlio/crates/s3dlio-oplog" }
```

### Step 4: Update Other Crates (5 min)
Update s3dlio dependency in:
- `crates/cli/Cargo.toml` line 22
- `crates/formats/Cargo.toml` line 18
- `crates/frameworks/Cargo.toml` line 13

**Same change**:
```toml
# Old:
s3dlio = { git = "https://github.com/russfellows/s3dlio.git", rev = "cd4ee2e" }

# New:
s3dlio = { git = "https://github.com/russfellows/s3dlio.git", tag = "v0.8.19" }
```

### Step 5: Test Compilation (5 min)
```bash
cargo clean
cargo build --release 2>&1 | tee build.log

# Expected: Clean build with possible warnings
# Any errors? Check build.log
```

### Step 6: Run Tests (10 min)
```bash
cargo test 2>&1 | tee test.log

# Expected: 61/61 tests passing
# Any failures? Check test.log and investigate
```

### Step 7: Verify CLI (2 min)
```bash
./target/release/dl-driver --help
./target/release/dl-driver replay --help

# Expected: Help text displays correctly
```

### Step 8: Commit Phase 1 (2 min)
```bash
git add Cargo.toml crates/*/Cargo.toml
git commit -m "Phase 1: Update s3dlio dependency to v0.8.19 and add s3dlio-oplog

- Update s3dlio from v0.8.7 (rev cd4ee2e) to v0.8.19
- Add s3dlio-oplog dependency for shared operation log functionality
- Remove aws-smithy-http-client patch (no longer needed)
- All tests passing (61/61)
"
git status
```

---

## 🔄 Next Steps After Phase 1

Once Phase 1 is complete and tested:

### Option A: Hybrid Approach (RECOMMENDED) 
Continue to Phase 2 in migration plan:
1. Add OpLogEntry ↔ OpLogRec conversion functions
2. Integrate s3dlio-oplog::OpLogReader for parsing
3. Keep SimpleReplayEngine with dl-driver features
4. Test thoroughly

### Option B: Pause and Report
If Phase 1 has issues:
1. Document the error in build.log or test.log
2. Check for version conflicts
3. Verify s3dlio-oplog path is correct
4. Consider using git dependency instead of path

---

## ⚠️ Common Issues & Solutions

### Issue: s3dlio-oplog not found
**Error**: `couldn't read .../s3dlio/crates/s3dlio-oplog/Cargo.toml`

**Solution**: Verify relative path or use git dependency:
```toml
# Instead of path dependency:
s3dlio-oplog = { git = "https://github.com/russfellows/s3dlio.git", tag = "v0.8.19" }
```

### Issue: Compilation errors
**Error**: Type mismatches or missing symbols

**Solution**: 
1. Check if all crates updated to v0.8.19
2. Run `cargo update`
3. Clear cache: `cargo clean`

### Issue: Test failures
**Error**: Some tests fail after update

**Solution**:
1. Check if tests use oplog functionality
2. Review test output carefully
3. May need Phase 2 changes before tests pass

---

## 📊 Success Indicators

After Phase 1 completion:

✅ **Green Indicators**:
- Zero compilation errors
- Minimal or zero warnings
- All dependency updates successful
- Build completes in < 2 minutes
- Tests run (even if some fail - expected before Phase 2)

⚠️ **Yellow Indicators** (acceptable):
- Some tests fail (expected - need Phase 2 integration)
- Deprecation warnings
- Unused import warnings

🔴 **Red Indicators** (need attention):
- Compilation errors
- Dependency resolution failures
- Segfaults or panics
- Complete test suite failure

---

## 🆘 Rollback Plan

If Phase 1 fails critically:

```bash
# Discard all changes
git checkout .
git clean -fd

# Or rollback specific files
git checkout Cargo.toml
git checkout crates/core/Cargo.toml
# etc.

# Rebuild with old dependencies
cargo clean
cargo build --release
```

---

## 📚 Reference Documentation

**Must Read**:
1. `HANDOFF_SUMMARY.md` - Context and overview
2. `MIGRATION_PLAN_S3DLIO_0.8.19.md` - Complete technical plan

**Optional Reference**:
3. `../s3dlio/docs/S3DLIO_OPLOG_INTEGRATION.md` - s3dlio-oplog API
4. `../s3dlio/docs/v0.8.19-RELEASE-NOTES.md` - What's new

**Source Code**:
5. `../s3dlio/crates/s3dlio-oplog/src/lib.rs` - API documentation
6. `crates/core/src/replay.rs` - Current implementation

---

## 💡 Pro Tips

1. **Test frequently**: Run `cargo test` after each major change
2. **Commit early**: Commit after each successful phase
3. **Read errors carefully**: Rust compiler errors are detailed and helpful
4. **Use clippy**: `cargo clippy --all-targets` for code quality
5. **Check warnings**: Fix warnings before proceeding to next phase
6. **Keep notes**: Document any deviations from the plan

---

## ⏱️ Time Estimates

- **Phase 1** (dependency updates): 30 minutes
- **Phase 2** (integration): 2-3 hours
- **Phase 3** (CLI updates): 1 hour
- **Phase 4** (testing): 1-2 hours
- **Phase 5** (documentation): 30 minutes
- **Total**: 5-7 hours for complete migration

---

## ✅ Ready to Start?

1. ✅ Read this quick start guide
2. ✅ Read HANDOFF_SUMMARY.md
3. ✅ Read MIGRATION_PLAN_S3DLIO_0.8.19.md
4. ✅ Create feature branch
5. ✅ Start with Phase 1 above

**Good luck! The detailed plan has all the answers you'll need.**

---

**Quick Start Version**: 1.0  
**Date**: October 3, 2025  
**Estimated Time**: 30 minutes for Phase 1