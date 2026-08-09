#!/usr/bin/env python3
"""Analyze English vocabulary in Grok Build documentation and Rust comments.

The script intentionally analyzes prose rather than identifiers or executable
code. It scans:

* the repository README;
* Markdown below crates/ (excluding generated/package wrapper content); and
* consecutive Rust line comments below crates/.

It prints a deterministic Markdown report to stdout. No third-party packages
or network access are required.
"""

from __future__ import annotations

import argparse
import collections
import dataclasses
import math
import re
import sys
from pathlib import Path
from typing import Iterator, Sequence


WORD_RE = re.compile(r"[A-Za-z][A-Za-z'-]*")
SENTENCE_BREAK_RE = re.compile(r"(?<=[.!?])\s+(?=[A-Z`])")
RUST_COMMENT_RE = re.compile(r"^\s*//(?P<doc>[/!]?)\s?(?P<text>.*)$")
MARKDOWN_LINK_RE = re.compile(r"\[([^]]+)]\([^)]+\)")

# Function words and prose words that do not help distinguish this project.
# Domain words such as state, value, type, request, and error stay eligible.
STOP_WORDS = {
    "about", "above", "after", "again", "against", "all", "also", "am", "an",
    "and", "another", "any", "are", "aren't", "around", "as", "at", "back", "be",
    "because", "been", "before", "being", "below", "between", "both", "but", "by",
    "can", "cannot", "could", "did", "do", "does", "doing", "done", "don't", "down",
    "during", "each", "either", "else", "enough", "even", "ever", "every", "few",
    "first", "for", "from", "further", "get", "gets", "getting", "got", "had", "has",
    "have", "having", "he", "her", "here", "hers", "herself", "him", "himself", "his",
    "how", "however", "if", "in", "into", "is", "isn't", "it", "its", "itself", "just",
    "last", "less", "let", "like", "made", "make", "makes", "many", "may", "me", "might",
    "more", "most", "much", "must", "my", "myself", "need", "needs", "neither", "never",
    "new", "no", "nor", "not", "now", "of", "off", "often", "on", "once", "one", "only",
    "or", "other", "our", "ours", "ourselves", "out", "over", "own", "same", "second",
    "see", "should", "since", "so", "some", "still", "such", "than", "that", "the", "their",
    "theirs", "them", "themselves", "then", "there", "these", "they", "this", "those",
    "through", "to", "too", "under", "unless", "until", "up", "use", "used", "using",
    "very", "via", "was", "we", "well", "were", "what", "when", "where", "whether",
    "which", "while", "who", "why", "will", "with", "within", "without", "would", "you",
    "your", "yours", "zero",
}

# Explicit aliases make word-family counts useful without pretending to be a
# full English lemmatizer. Ambiguous families remain visible in quoted context.
ALIASES = {
    "actors": "actor",
    "blocks": "block",
    "buffers": "buffer",
    "callers": "caller",
    "calls": "call",
    "channels": "channel",
    "clients": "client",
    "commands": "command",
    "configs": "config",
    "contexts": "context",
    "definitions": "definition",
    "errors": "error",
    "events": "event",
    "files": "file",
    "handles": "handle",
    "hooks": "hook",
    "images": "image",
    "inputs": "input",
    "items": "item",
    "messages": "message",
    "models": "model",
    "notifications": "notification",
    "outputs": "output",
    "permissions": "permission",
    "plugins": "plugin",
    "prompts": "prompt",
    "queries": "query",
    "requests": "request",
    "responses": "response",
    "results": "result",
    "retries": "retry",
    "servers": "server",
    "sessions": "session",
    "snapshots": "snapshot",
    "streams": "stream",
    "tasks": "task",
    "tests": "test",
    "threads": "thread",
    "tools": "tool",
    "turns": "turn",
    "updates": "update",
    "values": "value",
    "variants": "variant",
    "workflows": "workflow",
    "workspaces": "workspace",
}

EXCLUDED_PARTS = {
    ".git",
    "benches",
    "fixtures",
    "fuzz",
    "generated",
    "node_modules",
    "npm",
    "snapshots",
    "target",
    "testdata",
    "third_party",
}


@dataclasses.dataclass(frozen=True)
class Excerpt:
    path: str
    line: int
    kind: str
    text: str


@dataclasses.dataclass
class WordStats:
    occurrences: int = 0
    files: set[str] = dataclasses.field(default_factory=set)
    kind_counts: collections.Counter[str] = dataclasses.field(
        default_factory=collections.Counter
    )
    excerpts: list[Excerpt] = dataclasses.field(default_factory=list)

    @property
    def score(self) -> float:
        # Frequency finds core domain nouns; document spread prevents one large
        # module from dominating the ranking.
        return self.occurrences * (1.0 + math.log2(1 + len(self.files)))


@dataclasses.dataclass(frozen=True)
class CorpusStats:
    rust_files: int
    markdown_files: int
    excerpts: int
    tokens: int


def repository_root() -> Path:
    return Path(__file__).resolve().parents[3]


def is_excluded(path: Path) -> bool:
    return any(part in EXCLUDED_PARTS for part in path.parts)


def iter_corpus_files(root: Path) -> Iterator[tuple[Path, str]]:
    readme = root / "README.md"
    if readme.is_file():
        yield readme, "markdown"

    crates = root / "crates"
    if not crates.is_dir():
        return

    for path in sorted(crates.rglob("*")):
        if not path.is_file() or is_excluded(path.relative_to(root)):
            continue
        if path.suffix == ".rs":
            yield path, "rust-comment"
        elif path.suffix == ".md":
            yield path, "markdown"


def clean_prose(text: str) -> str:
    text = MARKDOWN_LINK_RE.sub(r"\1", text)
    return re.sub(r"\s+", " ", text).strip(" #>|-")


def split_sentences(text: str) -> list[str]:
    cleaned = clean_prose(text)
    if not cleaned:
        return []
    return [part.strip() for part in SENTENCE_BREAK_RE.split(cleaned) if part.strip()]


def rust_comment_excerpts(path: Path, root: Path) -> Iterator[Excerpt]:
    lines = path.read_text(encoding="utf-8", errors="replace").splitlines()
    buffer: list[str] = []
    start_line = 0
    buffer_kind = ""

    def flush() -> list[Excerpt]:
        if not buffer:
            return []
        paragraph = " ".join(part for part in buffer if part)
        return [
            Excerpt(str(path.relative_to(root)), start_line, buffer_kind, sentence)
            for sentence in split_sentences(paragraph)
        ]

    for line_number, line in enumerate(lines, 1):
        match = RUST_COMMENT_RE.match(line)
        if match:
            comment = match.group("text").strip()
            comment_kind = "rust-doc" if match.group("doc") else "rust-comment"
            if not comment:
                yield from flush()
                buffer = []
                start_line = 0
                buffer_kind = ""
                continue
            if buffer and comment_kind != buffer_kind:
                yield from flush()
                buffer = []
                start_line = 0
            if not buffer:
                start_line = line_number
                buffer_kind = comment_kind
            buffer.append(comment)
        else:
            yield from flush()
            buffer = []
            start_line = 0
            buffer_kind = ""
    yield from flush()


def markdown_excerpts(path: Path, root: Path) -> Iterator[Excerpt]:
    lines = path.read_text(encoding="utf-8", errors="replace").splitlines()
    buffer: list[str] = []
    start_line = 0
    in_fence = False

    def flush() -> list[Excerpt]:
        if not buffer:
            return []
        paragraph = " ".join(buffer)
        return [
            Excerpt(str(path.relative_to(root)), start_line, "markdown", sentence)
            for sentence in split_sentences(paragraph)
        ]

    for line_number, line in enumerate(lines, 1):
        stripped = line.strip()
        if stripped.startswith("```") or stripped.startswith("~~~"):
            yield from flush()
            buffer = []
            start_line = 0
            in_fence = not in_fence
            continue
        if in_fence:
            continue
        if stripped.startswith("|"):
            yield from flush()
            buffer = []
            start_line = 0
            continue
        if not stripped or stripped.startswith("<!--"):
            yield from flush()
            buffer = []
            start_line = 0
            continue
        if not buffer:
            start_line = line_number
        buffer.append(stripped)
    yield from flush()


def normalize_word(word: str) -> str:
    normalized = word.lower().strip("'-")
    if normalized.endswith("'s"):
        normalized = normalized[:-2]
    return ALIASES.get(normalized, normalized)


def words_in(text: str) -> Iterator[str]:
    for raw_word in WORD_RE.findall(text):
        word = normalize_word(raw_word)
        if len(word) >= 3 and word not in STOP_WORDS:
            yield word


def collect(root: Path) -> tuple[dict[str, WordStats], CorpusStats]:
    vocabulary: dict[str, WordStats] = collections.defaultdict(WordStats)
    rust_files = 0
    markdown_files = 0
    excerpt_count = 0
    token_count = 0

    for path, kind in iter_corpus_files(root):
        if kind == "rust-comment":
            rust_files += 1
            excerpts = rust_comment_excerpts(path, root)
        else:
            markdown_files += 1
            excerpts = markdown_excerpts(path, root)

        for excerpt in excerpts:
            excerpt_count += 1
            excerpt_words = list(words_in(excerpt.text))
            token_count += len(excerpt_words)
            counts = collections.Counter(excerpt_words)
            for word, count in counts.items():
                stats = vocabulary[word]
                stats.occurrences += count
                stats.files.add(excerpt.path)
                stats.kind_counts[excerpt.kind] += count
                stats.excerpts.append(excerpt)

    return vocabulary, CorpusStats(
        rust_files=rust_files,
        markdown_files=markdown_files,
        excerpts=excerpt_count,
        tokens=token_count,
    )


def select_words(
    vocabulary: dict[str, WordStats],
    requested: Sequence[str],
    top: int,
    min_files: int,
) -> list[tuple[str, WordStats]]:
    if requested:
        selected = []
        seen = set()
        for raw_word in requested:
            word = normalize_word(raw_word)
            if word in seen:
                continue
            seen.add(word)
            selected.append((word, vocabulary.get(word, WordStats())))
        return selected

    eligible = [
        (word, stats)
        for word, stats in vocabulary.items()
        if len(stats.files) >= min_files
    ]
    eligible.sort(
        key=lambda item: (-item[1].score, -len(item[1].files), item[0])
    )
    return eligible[:top]


def excerpt_quality(excerpt: Excerpt, word: str) -> tuple[int, int, int, int, int, str, int]:
    text = excerpt.text
    lower = text.lower()
    path_parts = Path(excerpt.path).parts
    filename = Path(excerpt.path).name.lower()
    test_penalty = int(
        "tests" in path_parts
        or filename in {"test.rs", "tests.rs"}
        or filename.endswith("_test.rs")
        or filename.endswith("_tests.rs")
    )
    definition_signal = int(
        any(
            signal in lower
            for signal in (
                f"{word} is ",
                f"{word} are ",
                f"{word} defines ",
                f"{word} owns ",
                f"{word} represents ",
                f"{word} contains ",
                f"{word} provides ",
                f"{word}: ",
            )
        )
    )
    test_language_penalty = int(
        any(signal in lower for signal in ("test that", "verify that", "simulate ", "assert "))
    )
    kind_priority = {"rust-doc": 0, "markdown": 1, "rust-comment": 2}.get(
        excerpt.kind, 3
    )
    length_penalty = int(not 45 <= len(text) <= 280)
    return (
        test_penalty,
        -definition_signal,
        test_language_penalty,
        kind_priority,
        length_penalty,
        excerpt.path,
        excerpt.line,
    )


def best_excerpts(stats: WordStats, word: str, limit: int) -> list[Excerpt]:
    candidates = []
    seen_text = set()
    for excerpt in stats.excerpts:
        if word not in set(words_in(excerpt.text)):
            continue
        key = excerpt.text.lower()
        if key in seen_text:
            continue
        seen_text.add(key)
        candidates.append(excerpt)
    candidates.sort(key=lambda excerpt: excerpt_quality(excerpt, word))
    return candidates[:limit]


def markdown_escape(text: str) -> str:
    return text.replace("|", "\\|").replace("\n", " ")


def render_report(
    selected: Sequence[tuple[str, WordStats]],
    corpus: CorpusStats,
    examples: int,
) -> str:
    lines = [
        "# Grok Build vocabulary analysis",
        "",
        "## Corpus",
        "",
        f"- Rust source files scanned: {corpus.rust_files}",
        f"- Markdown files scanned: {corpus.markdown_files}",
        f"- Prose excerpts analyzed: {corpus.excerpts}",
        f"- Non-stopword tokens analyzed: {corpus.tokens}",
        "",
        "The ranking combines occurrence count with file coverage. Rust numbers",
        "refer only to line comments; Markdown fenced code blocks are excluded.",
        "Selected aliases listed in the script are merged into a base form.",
        "",
        "## Words",
        "",
        "| Rank | Word | Occurrences | Files | Rust comments | Markdown | Score |",
        "| ---: | --- | ---: | ---: | ---: | ---: | ---: |",
    ]

    for rank, (word, stats) in enumerate(selected, 1):
        lines.append(
            "| {rank} | `{word}` | {occurrences} | {files} | {rust} | {markdown} | {score:.1f} |".format(
                rank=rank,
                word=word,
                occurrences=stats.occurrences,
                files=len(stats.files),
                rust=stats.kind_counts["rust-comment"] + stats.kind_counts["rust-doc"],
                markdown=stats.kind_counts["markdown"],
                score=stats.score,
            )
        )

    if examples:
        lines.extend(["", "## Original contexts", ""])
        for word, stats in selected:
            lines.append(f"### `{word}`")
            lines.append("")
            excerpts = best_excerpts(stats, word, examples)
            if not excerpts:
                lines.append("No matching prose excerpt found.")
                lines.append("")
                continue
            for excerpt in excerpts:
                text = markdown_escape(excerpt.text)
                lines.append(f"- `{excerpt.path}:{excerpt.line}` ({excerpt.kind}): {text}")
            lines.append("")

    return "\n".join(lines).rstrip() + "\n"


def parse_args(argv: Sequence[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Rank project vocabulary and quote original prose contexts."
    )
    parser.add_argument(
        "--root",
        type=Path,
        default=repository_root(),
        help="repository root (default: inferred from this script)",
    )
    parser.add_argument("--top", type=int, default=40, help="number of ranked words")
    parser.add_argument(
        "--min-files",
        type=int,
        default=3,
        help="minimum file coverage for automatic ranking",
    )
    parser.add_argument(
        "--examples",
        type=int,
        default=1,
        help="original contexts to show for each word (0 disables them)",
    )
    parser.add_argument(
        "--word",
        action="append",
        default=[],
        help="inspect one word instead of the automatic ranking; repeatable",
    )
    args = parser.parse_args(argv)
    if args.top < 1:
        parser.error("--top must be at least 1")
    if args.min_files < 1:
        parser.error("--min-files must be at least 1")
    if args.examples < 0:
        parser.error("--examples cannot be negative")
    return args


def main(argv: Sequence[str] | None = None) -> int:
    args = parse_args(argv if argv is not None else sys.argv[1:])
    root = args.root.resolve()
    if not (root / "crates").is_dir():
        print(f"error: not a Grok Build repository root: {root}", file=sys.stderr)
        return 2

    vocabulary, corpus = collect(root)
    selected = select_words(vocabulary, args.word, args.top, args.min_files)
    print(render_report(selected, corpus, args.examples), end="")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
