#!/bin/bash
# MielinOS Verification Script
# Runs comprehensive checks to verify project health

set -e

FAILED=0

echo "╔════════════════════════════════════════════════════════╗"
echo "║   MielinOS v0.0.1 Verification Script                 ║"
echo "╚════════════════════════════════════════════════════════╝"
echo ""

# Function to print status
print_status() {
    if [ $? -eq 0 ]; then
        echo "✅ $1"
    else
        echo "❌ $1"
        FAILED=$((FAILED + 1))
    fi
}

# 1. Check Rust version
echo "1️⃣  Checking Rust installation..."
rustc --version > /dev/null 2>&1
print_status "Rust compiler"

cargo --version > /dev/null 2>&1
print_status "Cargo"

# 2. Check required tools
echo ""
echo "2️⃣  Checking development tools..."

if command -v cargo-nextest &> /dev/null; then
    echo "✅ cargo-nextest"
else
    echo "⚠️  cargo-nextest (optional but recommended)"
fi

if command -v rustfmt &> /dev/null; then
    echo "✅ rustfmt"
else
    echo "❌ rustfmt (required)"
    FAILED=$((FAILED + 1))
fi

if command -v clippy-driver &> /dev/null; then
    echo "✅ clippy"
else
    echo "❌ clippy (required)"
    FAILED=$((FAILED + 1))
fi

# 3. Check WASM target
echo ""
echo "3️⃣  Checking WASM target..."
rustup target list | grep -q "wasm32-unknown-unknown (installed)"
print_status "wasm32-unknown-unknown target"

# 4. Build check
echo ""
echo "4️⃣  Checking build..."
cargo check --workspace --quiet > /dev/null 2>&1
print_status "Workspace compilation"

# 5. Format check
echo ""
echo "5️⃣  Checking code formatting..."
cargo fmt --all -- --check > /dev/null 2>&1
print_status "Code formatting"

# 6. Clippy check
echo ""
echo "6️⃣  Running Clippy..."
cargo clippy --workspace --quiet -- -D warnings > /dev/null 2>&1
print_status "Clippy lints"

# 7. Test check
echo ""
echo "7️⃣  Running tests..."
if command -v cargo-nextest &> /dev/null; then
    TEST_OUTPUT=$(cargo nextest run --workspace --no-fail-fast 2>&1)
    TEST_RESULT=$?
    if [ $TEST_RESULT -eq 0 ]; then
        PASSED=$(echo "$TEST_OUTPUT" | grep -o '[0-9]* passed' | grep -o '[0-9]*')
        echo "✅ Tests ($PASSED passed)"
    else
        echo "❌ Tests (some failed)"
        FAILED=$((FAILED + 1))
    fi
else
    cargo test --workspace --quiet > /dev/null 2>&1
    print_status "Tests"
fi

# 8. WASM build check
echo ""
echo "8️⃣  Checking WASM builds..."
cargo build -p counter-agent --target wasm32-unknown-unknown --release --quiet > /dev/null 2>&1
print_status "WASM agent build"

# 9. Documentation check
echo ""
echo "9️⃣  Checking documentation..."

docs=(
    "README.md"
    "TODO.md"
    "CONTRIBUTING.md"
    "CHANGELOG.md"
    "SECURITY.md"
    "CODE_OF_CONDUCT.md"
    "QUICKSTART.md"
    "PROJECT_STATUS.md"
    "LICENSE-MIT"
    "LICENSE-APACHE"
)

MISSING_DOCS=0
for doc in "${docs[@]}"; do
    if [ -f "$doc" ]; then
        :
    else
        echo "❌ Missing: $doc"
        MISSING_DOCS=$((MISSING_DOCS + 1))
    fi
done

if [ $MISSING_DOCS -eq 0 ]; then
    echo "✅ Core documentation (${#docs[@]} files)"
else
    echo "❌ Documentation ($MISSING_DOCS missing)"
    FAILED=$((FAILED + 1))
fi

# 10. Infrastructure check
echo ""
echo "🔟 Checking infrastructure..."

infra=(
    "Makefile"
    ".rustfmt.toml"
    ".editorconfig"
    ".gitignore"
    "Dockerfile"
    "docker-compose.yml"
    ".github/workflows/ci.yml"
    "scripts/setup.sh"
    "scripts/build.sh"
    "scripts/test.sh"
)

MISSING_INFRA=0
for file in "${infra[@]}"; do
    if [ -f "$file" ] || [ -d "$file" ]; then
        :
    else
        echo "❌ Missing: $file"
        MISSING_INFRA=$((MISSING_INFRA + 1))
    fi
done

if [ $MISSING_INFRA -eq 0 ]; then
    echo "✅ Infrastructure files (${#infra[@]} files)"
else
    echo "❌ Infrastructure ($MISSING_INFRA missing)"
    FAILED=$((FAILED + 1))
fi

# Summary
echo ""
echo "════════════════════════════════════════════════════════"
echo ""

if [ $FAILED -eq 0 ]; then
    echo "🎉 All checks passed! MielinOS v0.0.1 is healthy."
    echo ""
    echo "Quick commands:"
    echo "  make build    - Build all crates"
    echo "  make test     - Run all tests"
    echo "  make check    - Run quality checks"
    echo "  make examples - Run examples"
    echo ""
    exit 0
else
    echo "⚠️  $FAILED check(s) failed. Please review the output above."
    echo ""
    echo "Common fixes:"
    echo "  rustup component add rustfmt clippy"
    echo "  cargo install cargo-nextest"
    echo "  rustup target add wasm32-unknown-unknown"
    echo ""
    exit 1
fi
