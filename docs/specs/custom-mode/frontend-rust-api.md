# 前端共享库 API（Rust）

用途：TUI 复用的前端库，用于扫描/解析/校验/渲染/等价检测与去抖。通过 `ModeEngine` 与 `DataSource` 解耦数据源（本地文件/上游事件）。

核心类型（建议）
```rust
pub enum ModeKind { Persistent, Instant }
pub enum VarType { Text, Enum, Boolean, Number, Path }

pub struct ModeVariableDefinition { /* 见 frontmatter.md */ }
pub enum ModeScope { Project(String), Global }
pub struct ModeDefinition { /* 见 frontmatter.md */ }

pub struct EnabledMode<'a> {
    pub id: &'a str,
    pub display_name: Option<&'a str>,
    pub scope: &'a ModeScope,
    pub variables: indexmap::IndexMap<&'a str, Option<String>>, // None = UseDefault
}

pub struct RenderOptions {
    pub initial_indent: Option<String>,
    pub subsequent_indent: Option<String>,
}

#[derive(thiserror::Error, Debug)]
pub enum ModesError {
    #[error("illegal id: {0}")] IllegalId(String),            // E1001
    #[error("io error: {0}")] Io(String),                    // E1004
    #[error("parse frontmatter: {0}")] Frontmatter(String),  // E2001
    #[error("invalid enum for {0}")] EnumInvalid(String),    // E2102
    #[error("duplicate var {0}")] VarDup(String),            // E2101
    #[error("bad regex {0}")] Regex(String),                 // E2201
    #[error("unknown mode {0}")] UnknownMode(String),        // E1201
    #[error("validation failed: {0}")] Validation(String),   // E310x
    #[error("template error: {0}")] Template(String),        // E3201
}

pub trait DataSource {
    fn load_defs(&self) -> Result<indexmap::IndexMap<String, ModeDefinition>, ModesError>;
}

pub struct LocalFsDataSource<'a> {
    pub cwd: &'a std::path::Path,
    pub codex_home: Option<&'a std::path::Path>,
}

pub struct ModeEngine<D: DataSource> { /* 私有字段 */ }

impl<D: DataSource> ModeEngine<D> {
    pub fn new(ds: D) -> Self;
    pub fn defs(&self) -> &indexmap::IndexMap<String, ModeDefinition>;
    pub fn render_user_instructions(
        &self,
        base: &str,
        enabled: &[EnabledMode<'_>],
        opts: Option<&RenderOptions>,
    ) -> Result<String, ModesError>;
}

pub fn scan_modes(cwd: &std::path::Path, codex_home: Option<&std::path::Path>) -> Result<Vec<ModeDefinition>, ModesError>;
pub fn parse_slash_line(input: &str, defs: &[ModeDefinition]) -> Result<(String, indexmap::IndexMap<String, Option<String>>), ModesError>;
pub fn render_user_instructions(base: &str, enabled: &[EnabledMode<'_>], defs: &[ModeDefinition], opts: Option<&RenderOptions>) -> Result<String, ModesError>;

/// 规范化等价：CRLF→LF、去尾空格、空行折叠，比较规范化后的字节。
pub fn normalize_equiv(a: &str, b: &str) -> bool;

/// 简单等价（逐字节）。
pub fn is_equivalent(a: &str, b: &str) -> bool;

/// 简单去抖器：窗口内仅提交最后一次变更。
pub struct Debouncer { /* … */ }
impl Debouncer {
    pub fn new(window_ms: u64) -> Self;
    pub fn should_fire(&mut self) -> bool;
}
```

说明
- 所有对外函数返回结构化错误（含 code/message/hint）。
- 渲染无副作用；输出可直接用于 `OverrideTurnContext.user_instructions`。
- `normalize_equiv` 供 UI 做“等价短路”；`Debouncer` 供 UI 触发覆写时统一节流。
