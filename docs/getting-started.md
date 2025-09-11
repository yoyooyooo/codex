## Getting started

### CLI usage

| Command            | Purpose                            | Example                         |
| ------------------ | ---------------------------------- | ------------------------------- |
| `codex`            | Interactive TUI                    | `codex`                         |
| `codex "..."`      | Initial prompt for interactive TUI | `codex "fix lint errors"`       |
| `codex exec "..."` | Non-interactive "automation mode"  | `codex exec "explain utils.ts"` |

Key flags: `--model/-m`, `--ask-for-approval/-a`.

### Resuming interactive sessions

- Run `codex resume` to display the session picker UI
- Resume most recent: `codex resume --last`
- Resume by id: `codex resume <SESSION_ID>` (You can get session ids from /status or `~/.codex/sessions/`)

Examples:

```shell
# Open a picker of recent sessions
codex resume

# Resume the most recent session
codex resume --last

# Resume a specific session by id
codex resume 7f9f9a2e-1b3c-4c7a-9b0e-123456789abc
```

### Running with a prompt as input

You can also run Codex CLI with a prompt as input:

```shell
codex "explain this codebase to me"
```

```shell
codex --full-auto "create the fanciest todo-list app"
```

That's it - Codex will scaffold a file, run it inside a sandbox, install any
missing dependencies, and show you the live result. Approve the changes and
they'll be committed to your working directory.

### Quick: JSONL & “局部特性”一把梭

想要不进入交互界面、直接一次性拿到结果（如提示词优化/摘要/标题）？有两条常见路径：

- `codex exec --json`（快速原型）
  - 查看事件类型：
    ```bash
    echo 'hi' | codex exec --json - | jq -rc 'select(.msg) | .msg.type'
    ```
  - 只取完整 Agent 文本：
    ```bash
    echo '把这句改写更简洁' | codex exec --json - | jq -rc 'select(.msg.type=="agent_message") | .msg.message'
    ```
  - 拿最终完整输出（JSON 模式不打印 task_complete，本命令将最终文本写到文件）：
    ```bash
    tmp=$(mktemp); echo '把这句改写更简洁' | codex exec --json --output-last-message "$tmp" - >/dev/null; cat "$tmp"; rm "$tmp"
    ```
  - 只输出文本、禁用工具（按需附加）：
    ```bash
    -c 'base_instructions=禁止调用任何工具，只输出最终文本。' \
    -c include_plan_tool=false \
    -c include_apply_patch_tool=false \
    -c include_view_image_tool=false \
    -c tools.web_search_request=false
    ```

- `codex proto`（标准协议流，适合自建前端/服务集成）
  - 一次性提交并取最终文本：
    ```bash
    printf '%s\n' '{"id":"1","op":{"type":"user_input","items":[{"type":"text","text":"把这句改写更简洁"}]}}' \
      | codex proto \
      | jq -rc 'select(.msg.type=="task_complete") | .msg.last_agent_message'
    ```

更多细节与示例：
- 专题文档（与主会话无关的局部模型调用）：
  - 进程内（Rust）：docs/topics/local-capabilities/inprocess-rust.md
  - 跨语言（子进程/协议流）：docs/topics/local-capabilities/cross-language.md

### Example prompts

Below are a few bite-size examples you can copy-paste. Replace the text in quotes with your own task.

| ✨  | What you type                                                                   | What happens                                                               |
| --- | ------------------------------------------------------------------------------- | -------------------------------------------------------------------------- |
| 1   | `codex "Refactor the Dashboard component to React Hooks"`                       | Codex rewrites the class component, runs `npm test`, and shows the diff.   |
| 2   | `codex "Generate SQL migrations for adding a users table"`                      | Infers your ORM, creates migration files, and runs them in a sandboxed DB. |
| 3   | `codex "Write unit tests for utils/date.ts"`                                    | Generates tests, executes them, and iterates until they pass.              |
| 4   | `codex "Bulk-rename *.jpeg -> *.jpg with git mv"`                               | Safely renames files and updates imports/usages.                           |
| 5   | `codex "Explain what this regex does: ^(?=.*[A-Z]).{8,}$"`                      | Outputs a step-by-step human explanation.                                  |
| 6   | `codex "Carefully review this repo, and propose 3 high impact well-scoped PRs"` | Suggests impactful PRs in the current codebase.                            |
| 7   | `codex "Look for vulnerabilities and create a security review report"`          | Finds and explains security bugs.                                          |

### Memory with AGENTS.md

You can give Codex extra instructions and guidance using `AGENTS.md` files. Codex looks for `AGENTS.md` files in the following places, and merges them top-down:

1. `~/.codex/AGENTS.md` - personal global guidance
2. `AGENTS.md` at repo root - shared project notes
3. `AGENTS.md` in the current working directory - sub-folder/feature specifics

For more information on how to use AGENTS.md, see the [official AGENTS.md documentation](https://agents.md/).

### Tips & shortcuts

#### Use `@` for file search

Typing `@` triggers a fuzzy-filename search over the workspace root. Use up/down to select among the results and Tab or Enter to replace the `@` with the selected path. You can use Esc to cancel the search.

#### Image input

Paste images directly into the composer (Ctrl+V / Cmd+V) to attach them to your prompt. You can also attach files via the CLI using `-i/--image` (comma‑separated):

```bash
codex -i screenshot.png "Explain this error"
codex --image img1.png,img2.jpg "Summarize these diagrams"
```

#### Esc–Esc to edit a previous message

When the chat composer is empty, press Esc to prime “backtrack” mode. Press Esc again to open a transcript preview highlighting the last user message; press Esc repeatedly to step to older user messages. Press Enter to confirm and Codex will fork the conversation from that point, trim the visible transcript accordingly, and pre‑fill the composer with the selected user message so you can edit and resubmit it.

In the transcript preview, the footer shows an `Esc edit prev` hint while editing is active.

#### Shell completions

Generate shell completion scripts via:

```shell
codex completion bash
codex completion zsh
codex completion fish
```

#### `--cd`/`-C` flag

Sometimes it is not convenient to `cd` to the directory you want Codex to use as the "working root" before running Codex. Fortunately, `codex` supports a `--cd` option so you can specify whatever folder you want. You can confirm that Codex is honoring `--cd` by double-checking the **workdir** it reports in the TUI at the start of a new session.
