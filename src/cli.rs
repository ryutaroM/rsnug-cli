use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

/// AI エージェントから扱えるシークレット管理ツール
#[derive(Parser)]
#[command(name = "rsnug", version, about, arg_required_else_help = true)]
pub struct Cli {
    /// vault ファイルのパス
    #[arg(short = 'f', long, value_name = "PATH", global = true)]
    pub vault: Option<PathBuf>,

    /// 出力形式
    #[arg(long, value_enum, default_value_t = Format::Text, global = true)]
    pub format: Format,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
pub enum Format {
    Text,
    Json,
}

#[derive(Subcommand)]
pub enum Command {
    /// 新しい vault を作成する
    Init {
        /// 既存の vault を上書きする
        #[arg(long)]
        force: bool,
    },
    /// シークレットを設定する
    Set {
        key: String,

        /// 設定する値。省略する場合は --stdin が必要
        #[arg(required_unless_present = "stdin")]
        value: Option<String>,

        /// 値を標準入力から読み取る
        #[arg(long, conflicts_with = "value", required_unless_present = "value")]
        stdin: bool,
    },
    /// シークレットのメタ情報を取得する
    Get {
        key: String,

        /// 値を平文で出力する
        #[arg(long)]
        reveal: bool,
    },
    /// 登録されているキーの一覧を表示する
    List,
}

impl Command {
    pub fn name(&self) -> &'static str {
        match self {
            Command::Init { .. } => "init",
            Command::Set { .. } => "set",
            Command::Get { .. } => "get",
            Command::List => "list",
        }
    }
}
