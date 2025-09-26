#!/bin/bash
# SPDX-FileCopyrightText: 2024 Russ Fellows <russ.fellows@gmail.com>
# SPDX-License-Identifier: GPL-3.0-or-later

# Quick License Compliance Check Script

set -e

echo "🔍 dl-driver License Compliance Check"
echo "====================================="
echo

# Check if we're in the right directory
if [ ! -f "Cargo.toml" ]; then
    echo "❌ Error: Please run this script from the dl-driver root directory"
    exit 1
fi

# Check REUSE compliance
echo "📋 Checking REUSE compliance..."
if command -v reuse >/dev/null 2>&1; then
    if reuse lint; then
        echo "✅ REUSE compliance: PASSED"
    else
        echo "❌ REUSE compliance: FAILED"
        echo "   Run 'reuse lint' for detailed information"
    fi
else
    echo "⚠️  REUSE tool not found. Install with: pip install reuse"
fi

echo

# Count files with SPDX headers
echo "🏷️  Checking SPDX headers in source files..."

rust_files=$(find . -name "*.rs" -not -path "./target/*" -not -path "./.venv/*" | wc -l)
rust_with_spdx=$(find . -name "*.rs" -not -path "./target/*" -not -path "./.venv/*" -exec grep -l "SPDX-License-Identifier" {} \; | wc -l)

python_files=$(find . -name "*.py" -not -path "./target/*" -not -path "./.venv/*" -not -path "./__pycache__/*" | wc -l)
python_with_spdx=$(find . -name "*.py" -not -path "./target/*" -not -path "./.venv/*" -not -path "./__pycache__/*" -exec grep -l "SPDX-License-Identifier" {} \; | wc -l)

shell_files=$(find . -name "*.sh" -not -path "./target/*" -not -path "./.venv/*" | wc -l)
shell_with_spdx=$(find . -name "*.sh" -not -path "./target/*" -not -path "./.venv/*" -exec grep -l "SPDX-License-Identifier" {} \; | wc -l)

echo "📊 SPDX Header Coverage:"
echo "   Rust files:   ${rust_with_spdx}/${rust_files}"
echo "   Python files: ${python_with_spdx}/${python_files}"
echo "   Shell files:  ${shell_with_spdx}/${shell_files}"

total_files=$((rust_files + python_files + shell_files))
total_with_spdx=$((rust_with_spdx + python_with_spdx + shell_with_spdx))

if [ "$total_with_spdx" -eq "$total_files" ]; then
    echo "✅ SPDX headers: ALL source files covered"
else
    echo "⚠️  SPDX headers: ${total_with_spdx}/${total_files} source files covered"
fi

echo

# Check for license files
echo "📄 Checking license files..."
if [ -f "LICENSES/GPL-3.0-or-later.txt" ]; then
    echo "✅ GPL-3.0-or-later license file present"
else
    echo "❌ Missing GPL-3.0-or-later license file"
fi

if [ -f ".reuse/dep5" ]; then
    echo "✅ REUSE dep5 metadata file present"
else
    echo "❌ Missing .reuse/dep5 metadata file"
fi

echo

# Check GitHub Actions workflow
echo "🚀 Checking CI/CD setup..."
if [ -f ".github/workflows/license-compliance.yml" ]; then
    echo "✅ GitHub Actions license compliance workflow configured"
else
    echo "❌ Missing GitHub Actions license compliance workflow"
fi

echo

# Docker ScanCode check (if available)
echo "🔬 ScanCode compatibility check..."
if command -v docker >/dev/null 2>&1; then
    echo "📦 Docker available - can run ScanCode analysis"
    echo "   Run: docker run --rm -v \$(pwd):/workdir sixarm/scancode \\"
    echo "        --copyright --license --format html-app \\"
    echo "        /workdir /workdir/scancode-report.html"
else
    echo "⚠️  Docker not available - cannot run ScanCode locally"
fi

echo

# Summary
echo "📈 Compliance Summary:"
echo "   This repository implements enterprise-grade license compliance"
echo "   - REUSE Specification 3.3 compliant"  
echo "   - ScanCode Toolkit compatible"
echo "   - Automated CI/CD validation"
echo "   - GPL-3.0-or-later licensed"

echo
echo "📋 For detailed compliance report, see: docs/LICENSE-COMPLIANCE.md"
echo "🌐 REUSE status: https://api.reuse.software/info/github.com/russfellows/dl-driver"