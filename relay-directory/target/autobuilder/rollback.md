# Rollback Plan — relay-directory

## Strategy: revert_commit

Every commit on the `relay` workspace is `git revert`-clean. The `relay-directory`
crate has no schema migrations or persistent state that would make a revert unsafe.

## Steps

1. Identify the commit to revert:
   ```sh
   git -C ~/wintermute/relay log --oneline relay-directory/
   ```

2. Revert cleanly:
   ```sh
   git -C ~/wintermute/relay revert <sha> --no-edit
   ```

3. The SQLite store at `~/.local/share/relay/directory.db` is a local data file —
   no migration is needed on revert. If a schema-breaking change was reverted,
   delete the db and re-import from source files.

## Data safety

`relay directory import` is idempotent (upsert by stable id). Re-importing after a
revert is safe and will not create duplicate rows.
