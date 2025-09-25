# 文件发现 / ID / 优先级

搜索路径
- 从仓库根到 `cwd`（含）沿途的每一层目录的 `.codex/modes/` 目录。
- 追加 `$CODEX_HOME/modes/`（默认 `~/.codex/modes/`，若存在）。

过滤与 ID 生成
- 仅处理 `.md` 文件；路径段（目录名与文件名，不含扩展名）字符集限定为 `[A-Za-z0-9_-]`，否则跳过（E1001）。
- ID 为相对路径的段以 `:` 连接并前置 `/`：`a/b/c.md` → `/a:b:c`，`foo.md` → `/foo`。

合并与优先级
- 扫描顺序：仓库根 → … → `cwd` → `$CODEX_HOME/modes/`。
- 后写覆盖先写：距离 `cwd` 近的定义覆盖远处或全局定义；使用保序结构（如 `IndexMap`）保持插入顺序，用于稳定展示与渲染顺序。

错误与健壮性
- 无法读取时记录 E1004 并继续；非法 ID 记录 E1001 并跳过。

伪代码（扫描→合并→覆盖）
```
dirs = collect_ancestors_with(".codex/modes")  // 从根到 cwd
if env.CODEX_HOME/modes exists: dirs.push(env.CODEX_HOME/modes)

defs = IndexMap()
for dir in dirs:                 // 从优先级低到高
  for file in walk_md(dir):
    id = normalize_to_id(relpath(file, dir))   // a/b/c.md -> /a:b:c
    if illegal(id): log(E1001, file); continue
    defs.insert(id, parse(file))  // 后面的同 id 会覆盖前面的

return defs
```

路径合并示例
- 项目与全局同名覆盖：
  - `~/.codex/modes/review.md` 定义 `/review`
  - `<repo>/.codex/modes/review.md` 也定义 `/review`
  - 结果：`/review` 采用项目定义，Slash 标注为 `project:<reponame>`；全局定义视为被覆盖。
- 子目录命名空间：
  - `<repo>/.codex/modes/a/b/c.md` → `/a:b:c`
  - `<repo>/.codex/modes/a/c.md`   → `/a:c`
  - 两者互不覆盖。

ID 归一化细则
- 仅允许 `[A-Za-z0-9_-]`；其它字符（空格、额外冒号等）视为非法（E1001）并跳过。
- 扩展名必须是 `.md`；大小写敏感环境下仅接受小写 `.md`。

提交规范（避免与上游冲突）
- 禁止在“仓库根”提交 `.codex/modes/**` 与 `.codex/prompts/**` 示例文件：样例请放在 `docs/` 或 `tests/fixtures/`；
- 建议在 CI 加守卫，若检测到仓库根出现上述路径，直接失败；
- 产线/测试环境仅扫描项目内路径与 `$CODEX_HOME/modes/`，不依赖仓库根样例。
