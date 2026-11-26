# GEMINI.md

The instructions for working in this repository live in **[AGENTS.md](AGENTS.md)**. Read it
before making a change; it is short, and everything in it is load-bearing.

@AGENTS.md

It is kept in `AGENTS.md` rather than here so that every assistant reads the same file. Two
copies would drift, and the half that drifts is always the one someone forgot to update.

Before you touch anything, work in your own checkout, never the main one or another
assistant's:

```sh
sh tools/agent-worktree.sh gemini <branch>
```

and do everything from the directory it prints. [docs/PARALLEL_AGENTS.md](docs/PARALLEL_AGENTS.md)
explains why, and what is still shared: only one assistant runs the live app or a release at a
time.
