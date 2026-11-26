# Several assistants on one Mac

Goosic is worked on by more than one coding assistant at a time — Claude, Codex and Gemini —
often on the same machine and in the same hour. They read the same instructions
([AGENTS.md](../AGENTS.md), through `CLAUDE.md` and `GEMINI.md` for the two that look for
their own file), so they follow the same branch model. What this document adds is how they
share one computer without undoing each other's work.

## The problem is the checkout, not the branches

Branches never collide by themselves. What collides is a single working copy that two
assistants use at once. One runs `git switch` while the other is halfway through an edit, and
the edit lands on the wrong branch or is carried along as uncommitted changes. One starts
`make test` while the other's half-written file sits in the tree, and the run fails for a
reason neither of them caused. Both build into the same `target/` directory, where Cargo's
build lock makes the second one wait and a build of one branch throws away the other's. None of
this shows up as a conflict; it shows up as a strange failure an hour later.

## One worktree per assistant

So each assistant has a checkout of its own, a git worktree beside the main one:

```
~/Documents/GitHub/GoosicReborn           the main checkout, the person's own
~/Documents/GitHub/GoosicReborn-claude    Claude's
~/Documents/GitHub/GoosicReborn-codex     Codex's
~/Documents/GitHub/GoosicReborn-gemini    Gemini's
```

`tools/agent-worktree.sh` makes one and puts a branch in it:

```sh
sh tools/agent-worktree.sh codex fix/queue-undo           # new branch from development
sh tools/agent-worktree.sh gemini fix/queue-undo           # refused: Codex has it
sh tools/agent-worktree.sh claude hotfix/crash main        # a hotfix, cut from main
```

The worktree is kept from one task to the next rather than made per task. A cold Rust and
Swift build takes several minutes and a few gigabytes, and an assistant that keeps its own
worktree keeps its own warm `target/` and `.build/`. They are inside the worktree, so no two
assistants ever build into the same one. The Swift package already finds the shared rules
archive relative to its own checkout, so nothing needs to be configured for that.

Worktrees share one `.git` directory, which is what makes them cheap, and git refuses to
check out a branch that another worktree already has. That refusal is the rule that matters:
**one branch, one assistant**. When two of them need to work on the same change, one finishes
and pushes, and the other picks the branch up afterwards. The script says who holds a branch
instead of letting git's message be the first sign of it.

It also refuses to switch a worktree that has uncommitted work, and never checks out `main`,
which is deployment only.

## What is still shared

The live app is. A packaged or `make run-swift` Goosic keeps its settings, its accounts and
its signed-in WebKit profile in `~/Library/Application Support/goosic` and the user's WebKit
storage, wherever it was built from. Two copies running at once would write the same files
and the same sign-in. So **only one assistant runs the live app at a time**, and the tests,
which use temporary directories, are what an assistant runs by default. Making the data
location configurable per checkout would lift this, but it reaches into account storage, so it
is a change of its own.

A release is shared too. `tools/release.sh` works in a temporary worktree of its own, so it
does not disturb anyone's checkout, but it merges pull requests and moves `development`.
**One assistant runs a release at a time**, and the others leave `development` alone until it
says the deploy pull request is merged.

The main checkout belongs to the person. An assistant does not switch its branch, commit in
it, or leave files there; it works in its own worktree and pushes.

## Cleaning up

A finished branch is deleted from the remote by the pruning workflow. The worktree stays and
is simply switched to the next branch. To remove one entirely:

```sh
git worktree remove ../GoosicReborn-gemini
```

`git worktree prune` forgets worktrees whose directory was deleted by hand.
