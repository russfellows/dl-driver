#!/bin/bash
# Generate test op-log files for streaming replay tests
#
# This script creates sample operation logs in TSV format and compresses them
# with zstd for testing the streaming replay functionality.

set -e

OPLOG_DIR="tests/replay_tests/oplogs"

echo "🔧 Generating test op-log files..."

# Create directory if it doesn't exist
mkdir -p "$OPLOG_DIR"

# Function to create and compress an op-log file
create_oplog() {
    local filename="$1"
    local content="$2"
    
    echo "  Creating $filename..."
    echo -e "$content" > "$OPLOG_DIR/$filename.tsv"
    zstd -f -q "$OPLOG_DIR/$filename.tsv" -o "$OPLOG_DIR/$filename.csv.zst"
}

# Sample File Backend Op-log (10 operations)
create_oplog "sample_file_backend" \
"idx\top\tbytes\tendpoint\tfile\tstart\tduration_ns\terror
1\tPUT\t1048576\tfile:///tmp/replay_test\tdata/file_000001.bin\t2025-10-03T10:00:00Z\t50000000\t
2\tPUT\t1048576\tfile:///tmp/replay_test\tdata/file_000002.bin\t2025-10-03T10:00:01Z\t45000000\t
3\tGET\t1048576\tfile:///tmp/replay_test\tdata/file_000001.bin\t2025-10-03T10:00:02Z\t30000000\t
4\tGET\t1048576\tfile:///tmp/replay_test\tdata/file_000002.bin\t2025-10-03T10:00:03Z\t32000000\t
5\tLIST\t0\tfile:///tmp/replay_test\tdata/\t2025-10-03T10:00:04Z\t10000000\t
6\tSTAT\t0\tfile:///tmp/replay_test\tdata/file_000001.bin\t2025-10-03T10:00:05Z\t5000000\t
7\tSTAT\t0\tfile:///tmp/replay_test\tdata/file_000002.bin\t2025-10-03T10:00:06Z\t5000000\t
8\tDELETE\t0\tfile:///tmp/replay_test\tdata/file_000001.bin\t2025-10-03T10:00:07Z\t15000000\t
9\tDELETE\t0\tfile:///tmp/replay_test\tdata/file_000002.bin\t2025-10-03T10:00:08Z\t15000000\t
10\tLIST\t0\tfile:///tmp/replay_test\tdata/\t2025-10-03T10:00:09Z\t10000000\t"

# Sample S3 Backend Op-log (9 operations)
create_oplog "sample_s3_backend" \
"idx\top\tbytes\tendpoint\tfile\tstart\tduration_ns\terror
1\tPUT\t2097152\ts3://test-bucket\treplay/dataset_001.bin\t2025-10-03T11:00:00Z\t150000000\t
2\tPUT\t2097152\ts3://test-bucket\treplay/dataset_002.bin\t2025-10-03T11:00:02Z\t145000000\t
3\tGET\t2097152\ts3://test-bucket\treplay/dataset_001.bin\t2025-10-03T11:00:04Z\t80000000\t
4\tGET\t2097152\ts3://test-bucket\treplay/dataset_002.bin\t2025-10-03T11:00:05Z\t82000000\t
5\tLIST\t0\ts3://test-bucket\treplay/\t2025-10-03T11:00:06Z\t25000000\t
6\tSTAT\t0\ts3://test-bucket\treplay/dataset_001.bin\t2025-10-03T11:00:07Z\t20000000\t
7\tDELETE\t0\ts3://test-bucket\treplay/dataset_001.bin\t2025-10-03T11:00:08Z\t50000000\t
8\tDELETE\t0\ts3://test-bucket\treplay/dataset_002.bin\t2025-10-03T11:00:09Z\t50000000\t
9\tLIST\t0\ts3://test-bucket\treplay/\t2025-10-03T11:00:10Z\t25000000\t"

# Sample Azure Blob Backend Op-log (7 operations)
create_oplog "sample_azure_backend" \
"idx\top\tbytes\tendpoint\tfile\tstart\tduration_ns\terror
1\tPUT\t524288\taz://testcontainer\treplay/blob_001.bin\t2025-10-03T12:00:00Z\t200000000\t
2\tPUT\t524288\taz://testcontainer\treplay/blob_002.bin\t2025-10-03T12:00:02Z\t195000000\t
3\tGET\t524288\taz://testcontainer\treplay/blob_001.bin\t2025-10-03T12:00:04Z\t120000000\t
4\tLIST\t0\taz://testcontainer\treplay/\t2025-10-03T12:00:06Z\t40000000\t
5\tSTAT\t0\taz://testcontainer\treplay/blob_001.bin\t2025-10-03T12:00:07Z\t30000000\t
6\tDELETE\t0\taz://testcontainer\treplay/blob_001.bin\t2025-10-03T12:00:08Z\t60000000\t
7\tDELETE\t0\taz://testcontainer\treplay/blob_002.bin\t2025-10-03T12:00:09Z\t60000000\t"

# Sample GCS Backend Op-log (9 operations) - NEW in s3dlio 0.8.19
create_oplog "sample_gcs_backend" \
"idx\top\tbytes\tendpoint\tfile\tstart\tduration_ns\terror
1\tPUT\t1048576\tgs://test-gcs-bucket\treplay/object_001.bin\t2025-10-03T13:00:00Z\t180000000\t
2\tPUT\t1048576\tgs://test-gcs-bucket\treplay/object_002.bin\t2025-10-03T13:00:02Z\t175000000\t
3\tGET\t1048576\tgs://test-gcs-bucket\treplay/object_001.bin\t2025-10-03T13:00:04Z\t90000000\t
4\tGET\t1048576\tgs://test-gcs-bucket\treplay/object_002.bin\t2025-10-03T13:00:05Z\t92000000\t
5\tLIST\t0\tgs://test-gcs-bucket\treplay/\t2025-10-03T13:00:06Z\t30000000\t
6\tSTAT\t0\tgs://test-gcs-bucket\treplay/object_001.bin\t2025-10-03T13:00:07Z\t25000000\t
7\tDELETE\t0\tgs://test-gcs-bucket\treplay/object_001.bin\t2025-10-03T13:00:08Z\t55000000\t
8\tDELETE\t0\tgs://test-gcs-bucket\treplay/object_002.bin\t2025-10-03T13:00:09Z\t55000000\t
9\tLIST\t0\tgs://test-gcs-bucket\treplay/\t2025-10-03T13:00:10Z\t30000000\t"

# Sample DirectIO Backend Op-log (7 operations)
create_oplog "sample_directio_backend" \
"idx\top\tbytes\tendpoint\tfile\tstart\tduration_ns\terror
1\tPUT\t4194304\tdirect:///mnt/nvme\treplay_test/data_001.bin\t2025-10-03T14:00:00Z\t40000000\t
2\tPUT\t4194304\tdirect:///mnt/nvme\treplay_test/data_002.bin\t2025-10-03T14:00:01Z\t38000000\t
3\tGET\t4194304\tdirect:///mnt/nvme\treplay_test/data_001.bin\t2025-10-03T14:00:02Z\t25000000\t
4\tSTAT\t0\tdirect:///mnt/nvme\treplay_test/data_001.bin\t2025-10-03T14:00:03Z\t3000000\t
5\tDELETE\t0\tdirect:///mnt/nvme\treplay_test/data_001.bin\t2025-10-03T14:00:04Z\t8000000\t
6\tDELETE\t0\tdirect:///mnt/nvme\treplay_test/data_002.bin\t2025-10-03T14:00:05Z\t8000000\t
7\tLIST\t0\tdirect:///mnt/nvme\treplay_test/\t2025-10-03T14:00:06Z\t5000000\t"

# Real DirectIO test op-log (4 operations)
create_oplog "real_directio_test" \
"idx\top\tbytes\tendpoint\tfile\tstart\tduration_ns\terror
1\tPUT\t1048576\tdirect:///tmp/directio_replay_test\ttest_file_001.bin\t2025-10-03T19:00:00Z\t50000000\t
2\tGET\t1048576\tdirect:///tmp/directio_replay_test\ttest_file_001.bin\t2025-10-03T19:00:01Z\t45000000\t
3\tSTAT\t0\tdirect:///tmp/directio_replay_test\ttest_file_001.bin\t2025-10-03T19:00:02Z\t5000000\t
4\tDELETE\t0\tdirect:///tmp/directio_replay_test\ttest_file_001.bin\t2025-10-03T19:00:03Z\t10000000\t"

# Real S3 test op-log (5 operations)
create_oplog "real_s3_test" \
"idx\top\tbytes\tendpoint\tfile\tstart\tduration_ns\terror
1\tPUT\t524288\ts3://signal65-public\tdl-driver-test/replay_test_001.bin\t2025-10-03T19:00:00Z\t100000000\t
2\tGET\t524288\ts3://signal65-public\tdl-driver-test/replay_test_001.bin\t2025-10-03T19:00:01Z\t80000000\t
3\tSTAT\t0\ts3://signal65-public\tdl-driver-test/replay_test_001.bin\t2025-10-03T19:00:02Z\t20000000\t
4\tLIST\t0\ts3://signal65-public\tdl-driver-test/\t2025-10-03T19:00:03Z\t30000000\t
5\tDELETE\t0\ts3://signal65-public\tdl-driver-test/replay_test_001.bin\t2025-10-03T19:00:04Z\t50000000\t"

echo ""
echo "✅ Generated test op-log files in $OPLOG_DIR:"
echo "   - sample_file_backend (10 ops)"
echo "   - sample_s3_backend (9 ops)"
echo "   - sample_azure_backend (7 ops)"
echo "   - sample_gcs_backend (9 ops)"
echo "   - sample_directio_backend (7 ops)"
echo "   - real_directio_test (4 ops)"
echo "   - real_s3_test (5 ops)"
echo ""
echo "📦 Files created:"
ls -lh "$OPLOG_DIR"/*.csv.zst 2>/dev/null || echo "   (No compressed files yet - run with zstd installed)"
echo ""
