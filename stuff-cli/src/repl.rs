use clap::CommandFactory;
use tokio::io::{self, AsyncBufReadExt, AsyncWriteExt, BufReader};

use crate::{
    adapter::{CoreAdapter, ServiceError},
    cli::{Cli, Command},
};

pub async fn run(service: CoreAdapter, json: bool) -> Result<(), ServiceError> {
    let stdin = BufReader::new(io::stdin());
    let mut lines = stdin.lines();
    let mut stderr = io::stderr();
    write_prompt(&mut stderr).await?;

    while let Some(line) = lines.next_line().await.map_err(io_error)? {
        let line = line.trim();
        if is_exit(line) {
            break;
        }
        if let Some(path) = help_path(line) {
            stderr
                .write_all(help_text(&path)?.as_bytes())
                .await
                .map_err(io_error)?;
        } else if !line.is_empty() {
            match parse_line(line) {
                Ok(command) => {
                    if matches!(command, Command::Repl | Command::Init) {
                        super::print_error(
                            &ServiceError::Usage(
                                "init and repl can only be run from the system shell".into(),
                            ),
                            json,
                        );
                    } else if let Err(error) = super::dispatch(&service, command, json).await {
                        super::print_error(&error, json);
                    }
                }
                Err(error) => eprintln!("error: {error}"),
            }
        }
        write_prompt(&mut stderr).await?;
    }
    stderr.write_all(b"\n").await.map_err(io_error)?;
    Ok(())
}

async fn write_prompt(stderr: &mut io::Stderr) -> Result<(), ServiceError> {
    stderr.write_all(b"stuff> ").await.map_err(io_error)?;
    stderr.flush().await.map_err(io_error)
}

fn io_error(error: io::Error) -> ServiceError {
    ServiceError::Core(error.to_string())
}

fn is_exit(line: &str) -> bool {
    line.eq_ignore_ascii_case("quit") || line.eq_ignore_ascii_case("exit")
}

fn help_path(line: &str) -> Option<Vec<&str>> {
    let mut words = line.split_ascii_whitespace();
    match words.next() {
        Some("help" | "?") => Some(words.collect()),
        _ => None,
    }
}

fn help_text(path: &[&str]) -> Result<String, ServiceError> {
    let mut command = Cli::command();
    for name in path {
        command = command
            .find_subcommand(name)
            .cloned()
            .ok_or_else(|| ServiceError::Usage(format!("unknown command '{name}'")))?;
    }
    Ok(format!("{}\n", command.render_long_help()))
}

pub fn parse_line(line: &str) -> Result<Command, String> {
    let args = shell_words(line)?;
    if args.is_empty() {
        return Err("empty command".into());
    }
    if is_exit(&args[0]) {
        return Err("exit is handled by the interactive prompt".into());
    }
    let mut argv = vec!["stuff".to_owned()];
    argv.extend(args);
    let cli: Cli = clap::Parser::try_parse_from(argv).map_err(|error| error.to_string())?;
    if cli.json || cli.database_url.is_some() {
        return Err("pass --json or --database-url when starting the interactive prompt".into());
    }
    cli.command.ok_or_else(|| "enter a command".into())
}

fn shell_words(input: &str) -> Result<Vec<String>, String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut quote = None;
    let mut escaped = false;
    let mut started = false;
    for ch in input.chars() {
        if escaped {
            current.push(ch);
            escaped = false;
            started = true;
            continue;
        }
        if ch == '\\' && quote != Some('\'') {
            escaped = true;
            started = true;
            continue;
        }
        if let Some(q) = quote {
            if ch == q {
                quote = None;
            } else {
                current.push(ch);
            }
        } else if ch == '\'' || ch == '"' {
            quote = Some(ch);
            started = true;
        } else if ch.is_whitespace() {
            if started {
                words.push(std::mem::take(&mut current));
                started = false;
            }
        } else {
            current.push(ch);
            started = true;
        }
    }
    if escaped {
        return Err("unfinished escape".into());
    }
    if quote.is_some() {
        return Err("unterminated quote".into());
    }
    if started {
        words.push(current);
    }
    Ok(words)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::{AdjustArgs, StockCommand};

    #[test]
    fn parses_read_commands_and_quoted_locations() {
        assert_eq!(
            parse_line("find mcp6022").unwrap(),
            Command::Find {
                query: "mcp6022".into()
            }
        );
        assert_eq!(
            parse_line("contents \"Rack A / Shelf 1\"").unwrap(),
            Command::Contents {
                location: "Rack A / Shelf 1".into()
            }
        );
    }

    #[test]
    fn parses_nested_stock_commands() {
        assert_eq!(
            parse_line("stock adjust mcp6022 --by -2 --at 'Rack A / D4'").unwrap(),
            Command::Stock {
                command: StockCommand::Adjust(AdjustArgs {
                    item: "mcp6022".into(),
                    delta: -2,
                    location: "Rack A / D4".into(),
                })
            }
        );
    }

    #[test]
    fn shell_parser_handles_empty_quotes_and_literal_single_quote_backslashes() {
        assert_eq!(shell_words("contents \"\"").unwrap(), ["contents", ""]);
        assert_eq!(
            shell_words("find 'part\\name'").unwrap(),
            ["find", "part\\name"]
        );
        assert!(shell_words("find 'unfinished").is_err());
        assert!(shell_words("find trailing\\").is_err());
    }

    #[test]
    fn repl_meta_commands_and_help_are_clear() {
        assert!(is_exit("QUIT"));
        assert_eq!(help_path("help stock"), Some(vec!["stock"]));
        assert!(help_text(&["stock"]).unwrap().contains("add"));
        assert!(help_text(&["bogus"]).is_err());
        assert!(parse_line("quit").is_err());
        assert!(parse_line("find part --json").is_err());
    }
}
