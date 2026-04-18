use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SymbolKind {
    Func,
    Method,
    Struct,
    Interface,
    TypeAlias,
    Const,
    Var,
    Package,
}

impl SymbolKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            SymbolKind::Func => "func",
            SymbolKind::Method => "method",
            SymbolKind::Struct => "struct",
            SymbolKind::Interface => "interface",
            SymbolKind::TypeAlias => "type_alias",
            SymbolKind::Const => "const",
            SymbolKind::Var => "var",
            SymbolKind::Package => "package",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "func" => Some(SymbolKind::Func),
            "method" => Some(SymbolKind::Method),
            "struct" => Some(SymbolKind::Struct),
            "interface" => Some(SymbolKind::Interface),
            "type_alias" => Some(SymbolKind::TypeAlias),
            "const" => Some(SymbolKind::Const),
            "var" => Some(SymbolKind::Var),
            "package" => Some(SymbolKind::Package),
            _ => None,
        }
    }
}

impl std::fmt::Display for SymbolKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Symbol {
    pub id: Option<i64>,
    pub kind: SymbolKind,
    pub name: String,
    pub package: String,
    pub file: String,
    pub line: u32,
    pub col: u32,
    pub signature: Option<String>,
    pub doc: Option<String>,
    pub visibility: Visibility,
    pub hash: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Visibility {
    Exported,
    Private,
}

impl Visibility {
    pub fn from_name(name: &str) -> Self {
        name.chars()
            .next()
            .map(|c| {
                if c.is_uppercase() {
                    Visibility::Exported
                } else {
                    Visibility::Private
                }
            })
            .unwrap_or(Visibility::Private)
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Visibility::Exported => "exported",
            Visibility::Private => "private",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileRecord {
    pub path: String,
    pub hash: String,
    pub mtime: i64,
    pub parsed_at: i64,
    pub package: Option<String>,
}
