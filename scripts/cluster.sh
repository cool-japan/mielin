#!/bin/bash
# MielinOS 3-Node Mesh Cluster Management Script
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

cd "$PROJECT_ROOT"

# Colors for output
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m' # No Color

log_info() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Check if Docker is running
check_docker() {
    if ! docker info > /dev/null 2>&1; then
        log_error "Docker is not running. Please start Docker first."
        exit 1
    fi
}

# Start the cluster
start_cluster() {
    log_info "Starting MielinOS 3-node mesh cluster..."
    check_docker

    # Build and start services
    docker compose up -d core relay edge

    log_info "Cluster is starting..."
    log_info "Waiting for nodes to become healthy..."

    # Wait for health checks
    sleep 5

    # Show status
    docker compose ps

    log_info ""
    log_info "Cluster nodes:"
    log_info "  Core:  http://localhost:8080 (bootstrap node)"
    log_info "  Relay: http://localhost:8081 (connects to core)"
    log_info "  Edge:  http://localhost:8082 (connects to relay, has agent)"
    log_info ""
    log_info "To view logs: docker compose logs -f [core|relay|edge]"
    log_info "To stop:      $0 stop"
}

# Stop the cluster
stop_cluster() {
    log_info "Stopping MielinOS cluster..."
    docker compose down
    log_info "Cluster stopped."
}

# Restart the cluster
restart_cluster() {
    log_info "Restarting MielinOS cluster..."
    stop_cluster
    sleep 2
    start_cluster
}

# Show cluster status
status_cluster() {
    log_info "MielinOS Cluster Status:"
    docker compose ps

    echo ""
    log_info "Node Health:"

    # Check each node
    for node in core relay edge; do
        if docker compose ps $node | grep -q "Up"; then
            health=$(docker compose ps $node | grep "$node" | awk '{print $5}')
            echo "  $node: $health"
        else
            echo "  $node: Down"
        fi
    done
}

# Show logs for all or specific node
logs_cluster() {
    local node=$1
    if [ -z "$node" ]; then
        log_info "Showing logs for all nodes (Ctrl+C to exit)..."
        docker compose logs -f core relay edge
    else
        log_info "Showing logs for $node (Ctrl+C to exit)..."
        docker compose logs -f "$node"
    fi
}

# Execute command in a node
exec_node() {
    local node=$1
    shift
    local cmd=$@

    if [ -z "$node" ] || [ -z "$cmd" ]; then
        log_error "Usage: $0 exec <node> <command>"
        exit 1
    fi

    log_info "Executing in $node: $cmd"
    docker compose exec "$node" $cmd
}

# Clean up everything
clean_cluster() {
    log_warn "This will remove all containers, volumes, and images. Continue? (y/N)"
    read -r response
    if [[ "$response" =~ ^[Yy]$ ]]; then
        log_info "Cleaning up cluster..."
        docker compose down -v --rmi all
        log_info "Cleanup complete."
    else
        log_info "Cleanup cancelled."
    fi
}

# Build cluster images
build_cluster() {
    log_info "Building cluster images..."
    check_docker
    docker compose build core relay edge
    log_info "Build complete."
}

# Test migration between nodes
test_migration() {
    log_info "Testing agent migration from edge to core..."

    # First, ensure cluster is running
    if ! docker compose ps edge | grep -q "Up"; then
        log_error "Cluster is not running. Start it first with: $0 start"
        exit 1
    fi

    log_info "Triggering migration from edge node..."
    docker compose exec edge /app/node --migrate-to core:8080

    log_info "Check logs to verify migration:"
    log_info "  docker compose logs edge"
    log_info "  docker compose logs core"
}

# Show help
show_help() {
    cat << EOF
MielinOS 3-Node Mesh Cluster Management

Usage: $0 <command> [options]

Commands:
  start              Start the 3-node cluster
  stop               Stop the cluster
  restart            Restart the cluster
  status             Show cluster status
  logs [node]        Show logs (all nodes or specific: core, relay, edge)
  exec <node> <cmd>  Execute command in a node
  build              Build cluster Docker images
  clean              Remove all containers and volumes
  test-migration     Test agent migration from edge to core
  help               Show this help message

Examples:
  $0 start                    # Start the cluster
  $0 logs edge                # Show edge node logs
  $0 exec core /app/node --help
  $0 test-migration           # Test live migration

EOF
}

# Main command dispatcher
case "${1:-help}" in
    start)
        start_cluster
        ;;
    stop)
        stop_cluster
        ;;
    restart)
        restart_cluster
        ;;
    status)
        status_cluster
        ;;
    logs)
        logs_cluster "${2:-}"
        ;;
    exec)
        shift
        exec_node "$@"
        ;;
    build)
        build_cluster
        ;;
    clean)
        clean_cluster
        ;;
    test-migration)
        test_migration
        ;;
    help|--help|-h)
        show_help
        ;;
    *)
        log_error "Unknown command: $1"
        show_help
        exit 1
        ;;
esac
