# Install this skill

`npx skills` discovers any folder with `SKILL.md` (`name` + `description`) under
`skills/<name>/`. This directory already matches that layout.

## From GitHub (after this skill is on the default branch)

```bash
# list
npx skills add ashuralyk/ckb-cinnabar -l

# Cursor, user-level (all projects)
npx skills add ashuralyk/ckb-cinnabar -g -a cursor -s cinnabar-agent -y

# every detected agent
npx skills add ashuralyk/ckb-cinnabar -g -s cinnabar-agent --agent '*' -y
```

Shorthand that installs all skills in the repo:

```bash
npx skills add ashuralyk/ckb-cinnabar -g
```

Direct tree URL:

```bash
npx skills add https://github.com/ashuralyk/ckb-cinnabar/tree/main/skills/cinnabar-agent -g -a cursor -y
```

Until the skill is pushed, GitHub listing returns “No skills found”.

## From a local checkout (works now)

```bash
npx skills add /path/to/cinnabar -l
npx skills add /path/to/cinnabar -g -a cursor -s cinnabar-agent -y
```

## Manual copy

```bash
mkdir -p ~/.cursor/skills
cp -R skills/cinnabar-agent ~/.cursor/skills/cinnabar-agent
# Claude Code: cp -R skills/cinnabar-agent ~/.claude/skills/cinnabar-agent
```

The install copies this whole folder, including `binaries/` (Spore, Cluster,
xUDT, type-burn RISC-V scripts used in WarSporeSaga FakeRpc tests). Keep them
next to `SKILL.md` so agents can copy them into project `tests/binaries/` (or
`tests/fixtures/`).

After install, “写一个 CKB 锁仓合约” / “write a CKB lock script” should load
the skill even in an empty folder.

`npx skills find ckb` only lists skills already indexed on [skills.sh](https://skills.sh)
(typically after public installs). Direct `add owner/repo` does not need that index.
