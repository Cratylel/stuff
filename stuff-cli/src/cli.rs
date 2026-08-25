use clap::{Args, Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(
    name = "stuff",
    version,
    about = "Find items and manage physical stock",
    arg_required_else_help = false,
    disable_help_subcommand = true
)]
pub struct Cli {
    /// PostgreSQL connection URL (or set STUFF_DATABASE_URL).
    #[arg(long, env = "STUFF_DATABASE_URL", global = true)]
    pub database_url: Option<String>,

    /// Write command results as JSON.
    #[arg(long, global = true)]
    pub json: bool,

    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Debug, Clone, Subcommand, PartialEq)]
pub enum Command {
    /// Apply pending database migrations.
    Init,

    /// Find matching items, quantities, and every stock location.
    Find {
        /// Item name, part number, category, tag, or item UUID.
        query: String,
    },

    /// Show stock directly inside a location.
    Contents {
        /// Location name, path, or UUID.
        location: String,
    },

    /// Print the storage tree, optionally below one location.
    Tree {
        /// Location name, path, or UUID.
        selector: Option<String>,
    },

    /// List usable storage locations.
    Free(FreeArgs),

    /// Change stock quantities or move stock between locations.
    Stock {
        #[command(subcommand)]
        command: StockCommand,
    },

    /// Start the interactive prompt (also the default with no command).
    Repl,

    // Pre-0.1 compatibility. Hidden so the primary help has one taxonomy.
    #[command(hide = true)]
    Add(AddArgs),
    #[command(hide = true)]
    Adjust(AdjustArgs),
    #[command(hide = true)]
    Move(MoveArgs),
}

#[derive(Debug, Clone, Subcommand, PartialEq)]
pub enum StockCommand {
    /// Add an existing item to a location.
    Add(AddArgs),

    /// Apply a signed quantity change at one location.
    Adjust(AdjustArgs),

    /// Move some or all stock from one location to another.
    Move(MoveArgs),
}

#[derive(Debug, Clone, Args, PartialEq)]
pub struct FreeArgs {
    /// Only locations with zero estimated fill, no stock, and no reservation.
    #[arg(long)]
    pub completely_free: bool,

    /// Only locations whose estimated fill is at most this percentage.
    #[arg(long, value_name = "PERCENT", value_parser = parse_percent)]
    pub max_fill: Option<u8>,

    /// Only reserved locations that currently contain no stock.
    #[arg(long)]
    pub reserved_empty: bool,
}

#[derive(Debug, Clone, Args, PartialEq)]
pub struct AddArgs {
    /// Item name, part number, or item UUID.
    pub item: String,

    /// Number of pieces to add.
    #[arg(short, long, value_name = "COUNT", value_parser = positive_quantity)]
    pub quantity: i64,

    /// Destination location name, path, or UUID.
    #[arg(
        short = 'a',
        short_alias = 'l',
        long = "at",
        visible_alias = "location",
        value_name = "LOCATION"
    )]
    pub location: String,
}

#[derive(Debug, Clone, Args, PartialEq)]
pub struct AdjustArgs {
    /// Item name, part number, or item UUID.
    pub item: String,

    /// Signed quantity change, such as -2 or 5.
    #[arg(
        short = 'b',
        short_alias = 'q',
        long = "by",
        visible_alias = "quantity",
        value_name = "DELTA",
        allow_hyphen_values = true
    )]
    pub delta: i64,

    /// Location name, path, or UUID.
    #[arg(
        short = 'a',
        short_alias = 'l',
        long = "at",
        visible_alias = "location",
        value_name = "LOCATION"
    )]
    pub location: String,
}

#[derive(Debug, Clone, Args, PartialEq)]
pub struct MoveArgs {
    /// Item name, part number, or item UUID.
    pub item: String,

    /// Number of pieces. Omit to move all stock at the source.
    #[arg(short, long, value_name = "COUNT", value_parser = positive_quantity)]
    pub quantity: Option<i64>,

    /// Source location name, path, or UUID.
    #[arg(long, value_name = "LOCATION")]
    pub from: String,

    /// Destination location name, path, or UUID.
    #[arg(long, value_name = "LOCATION")]
    pub to: String,
}

fn parse_percent(value: &str) -> Result<u8, String> {
    let number = value
        .parse::<u8>()
        .map_err(|_| "expected a percentage from 0 to 100".to_owned())?;
    if number > 100 {
        Err("percentage must be between 0 and 100".to_owned())
    } else {
        Ok(number)
    }
}

fn positive_quantity(value: &str) -> Result<i64, String> {
    let quantity = value
        .parse::<i64>()
        .map_err(|_| "quantity must be a positive integer".to_owned())?;
    if quantity <= 0 {
        Err("quantity must be greater than zero".to_owned())
    } else {
        Ok(quantity)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::{CommandFactory, Parser};

    #[test]
    fn parses_global_options_before_and_after_subcommand() {
        let cli = Cli::try_parse_from(["stuff", "--json", "find", "mcp6022"]).unwrap();
        assert!(cli.json);
        assert_eq!(
            cli.command,
            Some(Command::Find {
                query: "mcp6022".into()
            })
        );

        let cli = Cli::try_parse_from(["stuff", "find", "mcp6022", "--json"]).unwrap();
        assert!(cli.json);
    }

    #[test]
    fn stock_commands_have_consistent_location_arguments() {
        let cli = Cli::try_parse_from([
            "stuff",
            "stock",
            "add",
            "MCP6022",
            "--quantity",
            "12",
            "--at",
            "Rack A / D4",
        ])
        .unwrap();
        assert_eq!(
            cli.command,
            Some(Command::Stock {
                command: StockCommand::Add(AddArgs {
                    item: "MCP6022".into(),
                    quantity: 12,
                    location: "Rack A / D4".into(),
                })
            })
        );

        let cli = Cli::try_parse_from([
            "stuff", "stock", "adjust", "MCP6022", "--by", "-2", "--at", "D4",
        ])
        .unwrap();
        assert_eq!(
            cli.command,
            Some(Command::Stock {
                command: StockCommand::Adjust(AdjustArgs {
                    item: "MCP6022".into(),
                    delta: -2,
                    location: "D4".into(),
                })
            })
        );

        let cli = Cli::try_parse_from([
            "stuff", "stock", "move", "MCP6022", "--from", "D4", "--to", "box",
        ])
        .unwrap();
        assert_eq!(
            cli.command,
            Some(Command::Stock {
                command: StockCommand::Move(MoveArgs {
                    item: "MCP6022".into(),
                    quantity: None,
                    from: "D4".into(),
                    to: "box".into(),
                })
            })
        );
    }

    #[test]
    fn accepts_hidden_legacy_mutation_commands_and_flags() {
        let cli = Cli::try_parse_from([
            "stuff",
            "adjust",
            "MCP6022",
            "--quantity",
            "-2",
            "--location",
            "D4",
        ])
        .unwrap();
        assert_eq!(
            cli.command,
            Some(Command::Adjust(AdjustArgs {
                item: "MCP6022".into(),
                delta: -2,
                location: "D4".into(),
            }))
        );

        let cli = Cli::try_parse_from(["stuff", "add", "MCP6022", "-q", "2", "-l", "D4"]).unwrap();
        assert_eq!(
            cli.command,
            Some(Command::Add(AddArgs {
                item: "MCP6022".into(),
                quantity: 2,
                location: "D4".into(),
            }))
        );
    }

    #[test]
    fn validates_quantities_and_fill_percentage() {
        assert!(Cli::try_parse_from(["stuff", "free", "--max-fill", "101"]).is_err());
        assert!(Cli::try_parse_from(["stuff", "free", "--max-fill", "25"]).is_ok());
        assert!(
            Cli::try_parse_from([
                "stuff",
                "stock",
                "add",
                "MCP6022",
                "--quantity",
                "0",
                "--at",
                "D4",
            ])
            .is_err()
        );
        assert!(
            Cli::try_parse_from([
                "stuff",
                "stock",
                "move",
                "MCP6022",
                "--quantity",
                "-1",
                "--from",
                "D4",
                "--to",
                "D5",
            ])
            .is_err()
        );
    }

    #[test]
    fn primary_help_has_one_inventory_lookup_and_one_stock_group() {
        let help = Cli::command().render_long_help().to_string();
        assert!(
            help.contains("find      Find matching items, quantities, and every stock location")
        );
        assert!(help.contains("stock     Change stock quantities or move stock between locations"));
        assert!(help.contains("repl      Start the interactive prompt"));
        assert!(!help.contains("Show all stock locations for an item"));
        assert!(!help.contains("\n  add "));
        assert!(!help.contains("\n  adjust "));
        assert!(!help.contains("\n  move "));
    }
}
