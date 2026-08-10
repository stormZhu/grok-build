#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd -- "$script_dir/../../.." && pwd)"
catalog="$script_dir/demo-catalog.tsv"
manifest="$script_dir/async-demos/Cargo.toml"
bin_dir="$script_dir/async-demos/src/bin"

usage() {
    cat <<'EOF'
用法：docs/rust-essentials/labs/study.sh <command> [name]

命令：
  list [track]  列出全部实验，或只列一条学习路径
  tracks        列出学习路径及实验数量
  show <bin>    显示实验目标、源码、精读和完成证据
  checklist [track]
                输出全部实验或一条路径的 Markdown 检查表
  run <bin>     运行一个实验
  path <track>  按顺序运行一条学习路径
  all           运行 Katas、compile-fail 和全部 async demos
  help          显示本帮助
EOF
}

fail() {
    echo "study.sh: $*" >&2
    exit 1
}

is_known_track() {
    case "$1" in
        async-core | turn | tool-safety | state-recovery | runtime-assembly | integration)
            return 0
            ;;
        *)
            return 1
            ;;
    esac
}

validate_catalog() {
    [[ -r "$catalog" ]] || fail "找不到目录：$catalog"

    local seen=$'\n'
    local demo track focus reading
    local count=0
    while IFS=$'\t' read -r demo track focus reading; do
        [[ -z "$demo" || "$demo" == \#* ]] && continue
        [[ -n "$track" && -n "$focus" && -n "$reading" ]] || fail "目录字段不完整：$demo"
        is_known_track "$track" || fail "未知学习路径：$track"
        [[ "$seen" != *$'\n'"$demo"$'\n'* ]] || fail "重复实验：$demo"
        [[ -f "$bin_dir/$demo.rs" ]] || fail "实验源码不存在：$demo.rs"
        [[ -f "$repo_root/$reading" ]] || fail "精读文档不存在：$reading"
        seen+="$demo"$'\n'
        count=$((count + 1))
    done < "$catalog"

    local source_count
    source_count="$(find "$bin_dir" -maxdepth 1 -type f -name '*.rs' | wc -l | tr -d '[:space:]')"
    [[ "$count" -eq "$source_count" ]] || fail "目录有 $count 项，但 bin 目录有 $source_count 个源码文件"
}

load_demo() {
    local wanted="$1"
    local demo track focus reading
    demo_track=""
    demo_focus=""
    demo_reading=""
    while IFS=$'\t' read -r demo track focus reading; do
        if [[ "$demo" == "$wanted" ]]; then
            demo_track="$track"
            demo_focus="$focus"
            demo_reading="$reading"
            return 0
        fi
    done < "$catalog"
    return 1
}

print_tracks() {
    awk -F '\t' '
        !/^#/ && NF >= 4 {
            if (!seen[$2]++) order[++count] = $2
            totals[$2]++
        }
        END {
            for (i = 1; i <= count; i++) {
                track = order[i]
                printf "%-18s %d 个实验\n", track, totals[track]
            }
        }
    ' "$catalog"
}

list_demos() {
    local wanted_track="${1:-}"
    if [[ -n "$wanted_track" ]]; then
        is_known_track "$wanted_track" || fail "未知学习路径：$wanted_track"
    fi

    local demo track focus reading current_track=""
    while IFS=$'\t' read -r demo track focus reading; do
        [[ -z "$demo" || "$demo" == \#* ]] && continue
        [[ -z "$wanted_track" || "$track" == "$wanted_track" ]] || continue
        if [[ "$track" != "$current_track" ]]; then
            [[ -z "$current_track" ]] || echo
            echo "[$track]"
            current_track="$track"
        fi
        printf '  %-29s %s\n' "$demo" "$focus"
    done < "$catalog"
}

show_demo() {
    local demo="$1"
    load_demo "$demo" || fail "未知实验：$demo"
    printf '实验：%s\n' "$demo"
    printf '路径：%s\n' "$demo_track"
    printf '目标：%s\n' "$demo_focus"
    printf '源码：docs/rust-essentials/labs/async-demos/src/bin/%s.rs\n' "$demo"
    printf '精读：%s\n' "$demo_reading"
    printf '运行：docs/rust-essentials/labs/study.sh run %s\n' "$demo"
    echo "证据：解释断言，并写出 owner、边界、失败分支和一条源码或测试证据"
}

run_demo() {
    local demo="$1"
    load_demo "$demo" || fail "未知实验：$demo"
    echo
    echo "==> $demo [$demo_track]"
    echo "预测：$demo_focus"
    cargo run --quiet --locked --manifest-path "$manifest" --bin "$demo"
    echo "下一步精读：$demo_reading"
    echo "完成前：写出 owner、边界、失败分支和一条源码或测试证据"
}

print_checklist() {
    local wanted_track="${1:-}"
    if [[ -n "$wanted_track" ]]; then
        is_known_track "$wanted_track" || fail "未知学习路径：$wanted_track"
    fi

    echo "# Grok Build demo 学习检查表"
    echo
    echo "只有在完成预测、运行、精读和证据记录后才勾选。"

    local demo track focus reading current_track=""
    while IFS=$'\t' read -r demo track focus reading; do
        [[ -z "$demo" || "$demo" == \#* ]] && continue
        [[ -z "$wanted_track" || "$track" == "$wanted_track" ]] || continue
        if [[ "$track" != "$current_track" ]]; then
            echo
            echo "## $track"
            echo
            current_track="$track"
        fi
        printf -- '- [ ] `%s`：%s\n' "$demo" "$focus"
        printf '  - 精读：`%s`\n' "$reading"
        echo "  - 证据：预测、owner、边界、失败分支、运行或测试结果"
    done < "$catalog"
}

run_path() {
    local wanted_track="$1"
    is_known_track "$wanted_track" || fail "未知学习路径：$wanted_track"

    local demo track focus reading
    local demos=()
    while IFS=$'\t' read -r demo track focus reading; do
        [[ -z "$demo" || "$demo" == \#* ]] && continue
        [[ "$track" == "$wanted_track" ]] || continue
        demos+=("$demo")
    done < "$catalog"
    [[ "${#demos[@]}" -gt 0 ]] || fail "学习路径没有实验：$wanted_track"

    for demo in "${demos[@]}"; do
        run_demo "$demo"
    done
}

validate_catalog

command="${1:-list}"
case "$command" in
    list)
        [[ $# -le 2 ]] || fail "list 最多接受一个学习路径名"
        list_demos "${2:-}"
        ;;
    tracks)
        [[ $# -le 1 ]] || fail "tracks 不接受额外参数"
        print_tracks
        ;;
    show)
        [[ $# -eq 2 ]] || fail "show 需要一个 bin 名"
        show_demo "$2"
        ;;
    checklist)
        [[ $# -le 2 ]] || fail "checklist 最多接受一个学习路径名"
        print_checklist "${2:-}"
        ;;
    run)
        [[ $# -eq 2 ]] || fail "run 需要一个 bin 名"
        run_demo "$2"
        ;;
    path)
        [[ $# -eq 2 ]] || fail "path 需要一个学习路径名"
        run_path "$2"
        ;;
    all)
        [[ $# -eq 1 ]] || fail "all 不接受额外参数"
        exec "$script_dir/check.sh"
        ;;
    help | -h | --help)
        [[ $# -le 1 ]] || fail "help 不接受额外参数"
        usage
        ;;
    *)
        usage >&2
        fail "未知命令：$command"
        ;;
esac
