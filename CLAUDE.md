## Version Control

This project uses **jj (Jujutsu)** for version control, not git directly.

### Committing Changes

Use `jj commit` unless more complicated flow is required:

```bash
jj commit -m "<type>: <description>"
```

### Commit Message Format

Follow **Conventional Commits** specification:

```
<type>[optional scope]: <description>
```

**Types:**
- `feat` - new feature
- `fix` - bug fix
- `refactor` - code change that neither fixes a bug nor adds a feature
- `docs` - documentation only
- `test` - adding or updating tests
- `chore` - maintenance tasks, dependencies, tooling
- `perf` - performance improvement
- `style` - formatting, whitespace (not CSS)
- `build` - build system or external dependencies
- `ci` - CI/CD configuration

**Examples:**
```bash
jj commit -m "feat: add pattern matching to parser"
jj commit -m "fix: resolve unification failure with recursive types"
jj commit -m "refactor(codegen): simplify JS emission for let bindings"
jj commit -m "docs: update README with build instructions"
```

**Breaking changes:** Add `!` after type:
```bash
jj commit -m "refactor!: rename Expr to Expression in AST"
```

### Common jj Commands

```bash
jj status          # show working copy status
jj log             # view commit history
jj diff            # show changes in working copy
jj commit -m "..." # commit with message
jj describe -m "..." # change message of current working copy commit
jj new             # start a new change on top of current
jj squash          # squash into parent commit
```

### Guidelines

- Keep commits focused and atomic
- Write descriptions in imperative mood ("add feature" not "added feature")
- Keep the description line under 72 characters
- Use scope sparingly, only when it adds clarity
