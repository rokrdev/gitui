# GitUI fork custom features

This fork follows the same branch convention as the personal Helix fork:

- `master` mirrors upstream GitUI.
- `custom` contains personal features and is the branch to build/install.

## Worktree browser

Press `6` to open the **Worktrees** tab. It lists the main checkout and every
registered linked worktree, including each path, branch, HEAD, and state.

Move with the normal list navigation keys and press `Enter` to open the selected
worktree. GitUI reopens the repository at that path so status and staging use
the selected worktree's own index.

The direct tab key can be customized with `tab_worktrees` in
`key_bindings.ron`.

## Open a commit from the command line

Pass a commit hash or other commit revision as the positional argument:

```sh
gitui d34db33
gitui HEAD~2
```

GitUI starts with its existing commit-inspection view open, showing the commit
metadata, changed files, and diff. An invalid revision exits with an error.
