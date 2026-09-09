#!/bin/bash
# CC Switch 开发/调试脚本
# 注意：生产环境请使用 .deb 安装包以获取原生桌面窗口体验。
#       本脚本仅用于开发和调试目的。
#
# 用法:
#   ./run-web.sh             构建并启动（前台运行）
#   ./run-web.sh start       后台启动
#   ./run-web.sh stop        停止服务
#   ./run-web.sh restart     重启服务
#   ./run-web.sh status      查看状态
#   ./run-web.sh logs        查看日志（仅后台模式）
#   ./run-web.sh package     构建 .deb 安装包

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
cd "$SCRIPT_DIR"

PID_FILE="/tmp/cc-switch-web.pid"
LOG_FILE="/tmp/cc-switch-web.log"

print_usage() {
    echo "用法: $0 [start|stop|restart|status|logs|package]"
    echo ""
    echo "  无参数      构建并启动（前台运行）"
    echo "  start       后台启动"
    echo "  stop        停止服务"
    echo "  restart     重启服务"
    echo "  status      查看运行状态"
    echo "  logs        查看日志（后台模式）"
    echo "  package     构建 .deb 安装包"
}

# 构建前后端
build_all() {
    echo "=== Building web-server ==="
    cargo build -p web-server 2>&1 | tail -3

    echo "=== Building frontend ==="
    pnpm build:renderer 2>&1 | tail -3

    echo "✓ Build complete"
}

# 检查是否正在运行
is_running() {
    if [ -f "$PID_FILE" ]; then
        local pid
        pid=$(cat "$PID_FILE" 2>/dev/null)
        if [ -n "$pid" ] && kill -0 "$pid" 2>/dev/null; then
            return 0
        fi
        # PID 文件存在但进程已死，清理
        rm -f "$PID_FILE"
    fi
    # 也检查是否有进程占用端口
    if command -v ss &>/dev/null; then
        ss -tlnp | grep -q ":2891" && return 0
    fi
    return 1
}

# 停止服务
do_stop() {
    local pids
    # 从 PID 文件停止
    if [ -f "$PID_FILE" ]; then
        local pid
        pid=$(cat "$PID_FILE" 2>/dev/null)
        if [ -n "$pid" ]; then
            echo "Stopping web-server (PID: $pid)..."
            kill "$pid" 2>/dev/null || true
            # 等待进程退出
            for i in $(seq 1 10); do
                if ! kill -0 "$pid" 2>/dev/null; then
                    break
                fi
                sleep 0.3
            done
            # 强制终止
            kill -9 "$pid" 2>/dev/null || true
        fi
        rm -f "$PID_FILE"
    fi

    # 清理所有 web-server 进程
    pids=$(pgrep -f "target/debug/web-server" 2>/dev/null || true)
    if [ -n "$pids" ]; then
        echo "Killing remaining web-server processes..."
        kill $pids 2>/dev/null || true
        sleep 1
        pids=$(pgrep -f "target/debug/web-server" 2>/dev/null || true)
        if [ -n "$pids" ]; then
            kill -9 $pids 2>/dev/null || true
        fi
    fi

    echo "✓ Web-server stopped"
}

# 前台启动（默认）
do_run() {
    build_all
    echo ""
    echo "Starting web-server at http://127.0.0.1:2891"
    echo "Press Ctrl+C to stop."
    echo ""
    exec ./target/debug/web-server
}

# 后台启动
do_start() {
    build_all

    if is_running; then
        echo "Web-server is already running. Use '$0 restart' to restart."
        return 1
    fi

    echo "Starting web-server in background..."
    nohup ./target/debug/web-server > "$LOG_FILE" 2>&1 &
    local pid=$!
    echo "$pid" > "$PID_FILE"

    # 等待服务就绪
    sleep 2
    if kill -0 "$pid" 2>/dev/null; then
        echo "✓ Web-server started (PID: $pid)"
        echo "  URL: http://127.0.0.1:2891"
        echo "  Logs: $LOG_FILE"
        echo "  Use '$0 logs' to view logs"
    else
        echo "✗ Web-server failed to start"
        cat "$LOG_FILE" | tail -5
        rm -f "$PID_FILE"
        return 1
    fi
}

# 查看状态
do_status() {
    if [ -f "$PID_FILE" ]; then
        local pid
        pid=$(cat "$PID_FILE" 2>/dev/null)
        if [ -n "$pid" ] && kill -0 "$pid" 2>/dev/null; then
            echo "Web-server is running (PID: $pid)"
            echo "URL: http://127.0.0.1:2891"
            return 0
        fi
        rm -f "$PID_FILE"
    fi

    # 检查端口
    if command -v ss &>/dev/null && ss -tlnp | grep -q ":2891"; then
        local pid
        pid=$(ss -tlnp | grep ":2891" | grep -oP 'pid=\K[0-9]+' || true)
        if [ -n "$pid" ]; then
            echo "Web-server is running (PID: $pid, no PID file)"
            return 0
        fi
    fi

    echo "Web-server is NOT running"
    return 1
}

# 查看日志
do_logs() {
    if [ ! -f "$LOG_FILE" ]; then
        echo "No log file found."
        echo "Start with '$0 start' first."
        return 1
    fi
    tail -f "$LOG_FILE"
}

# ====== 主入口 ======

case "${1:-}" in
    start)
        do_start
        ;;
    stop)
        do_stop
        ;;
    restart)
        do_stop
        sleep 1
        do_start
        ;;
    status)
        do_status
        ;;
    logs)
        do_logs
        ;;
    -h|--help)
        print_usage
        ;;
    "")
        do_run
        ;;
    package)
        echo "=== Building .deb package ==="
        exec ./build-deb.sh
        ;;
    *)
        echo "Unknown command: $1"
        print_usage
        exit 1
        ;;
esac
