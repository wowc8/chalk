# Tauri + React + Typescript

This template should help get you started developing with Tauri, React and Typescript in Vite.

## Recommended IDE Setup

- [VS Code](https://code.visualstudio.com/) + [Tauri](https://marketplace.visualstudio.com/items?itemName=tauri-apps.tauri-vscode) + [rust-analyzer](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust-analyzer)

## Viewing Logs

Chalk writes structured JSON logs to a daily rolling file. The default log level is `info`.

**Log file location (macOS):**

```
~/Library/Application Support/com.madison.chalk/logs/
```

### Tail logs in real time

```bash
tail -f ~/Library/Application\ Support/com.madison.chalk/logs/*.log | jq .
```

### Enable debug logging during development

Set the `RUST_LOG` environment variable before starting the dev server:

```bash
RUST_LOG=debug make dev
```

### Filter logs by module

Use `RUST_LOG` to target specific modules:

```bash
# Only chat module at debug level
RUST_LOG=chalk_lib::chat=debug make dev

# Multiple modules
RUST_LOG=chalk_lib::chat=debug,chalk_lib::digest=debug make dev

# Everything at debug except noisy dependencies
RUST_LOG=debug,hyper=info,reqwest=info make dev
```

Common module paths: `chalk_lib::chat`, `chalk_lib::digest`, `chalk_lib::connectors`, `chalk_lib::database`, `chalk_lib::admin`.
