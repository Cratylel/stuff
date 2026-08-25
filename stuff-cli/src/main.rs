mod adapter;
mod cli;
mod format;
mod repl;

use std::process::ExitCode;

use clap::Parser;
use serde::Serialize;

use crate::adapter::{CoreAdapter, Service, ServiceError};
use crate::cli::{Cli, Command, StockCommand};

#[derive(Serialize)]
struct InitResult {
    operation: &'static str,
    ok: bool,
}

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    let json = cli.json;

    if matches!(cli.command.as_ref(), Some(Command::Init)) {
        let Some(database_url) = cli.database_url.as_deref() else {
            return report_error(ServiceError::MissingDatabaseUrl, json);
        };
        return match CoreAdapter::migrate(database_url).await {
            Ok(()) => {
                if json {
                    print!(
                        "{}",
                        format::json_output(&InitResult {
                            operation: "init",
                            ok: true,
                        })
                    );
                } else {
                    println!("Database initialized.");
                }
                ExitCode::SUCCESS
            }
            Err(error) => report_error(error, json),
        };
    }

    if json && matches!(cli.command.as_ref(), None | Some(Command::Repl)) {
        return report_error(
            ServiceError::Usage("--json cannot be used with the interactive prompt".into()),
            json,
        );
    }

    let service = match CoreAdapter::connect(cli.database_url.as_deref()).await {
        Ok(service) => service,
        Err(error) => return report_error(error, json),
    };

    let result = match cli.command {
        None | Some(Command::Repl) => repl::run(service, json).await,
        Some(command) => dispatch(&service, command, json).await,
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => report_error(error, json),
    }
}

async fn dispatch(service: &CoreAdapter, command: Command, json: bool) -> Result<(), ServiceError> {
    use Command::*;

    match command {
        Init => unreachable!("init is handled before connecting the service"),
        Repl => unreachable!("repl is handled before command dispatch"),
        Find { query } => print!("{}", format::find(service.find(&query).await?, json)),
        Contents { location } => {
            print!(
                "{}",
                format::contents(service.contents(&location).await?, json)
            )
        }
        Tree { selector } => print!(
            "{}",
            format::tree(service.tree(selector.as_deref()).await?, json)
        ),
        Free(args) => print!(
            "{}",
            format::free(
                service
                    .free(adapter::FreeFilter {
                        completely_free: args.completely_free,
                        max_fill: args.max_fill,
                        reserved_empty: args.reserved_empty,
                    })
                    .await?,
                json,
            )
        ),
        Stock { command } => dispatch_stock(service, command, json).await?,
        Add(args) => dispatch_stock(service, StockCommand::Add(args), json).await?,
        Adjust(args) => dispatch_stock(service, StockCommand::Adjust(args), json).await?,
        Move(args) => dispatch_stock(service, StockCommand::Move(args), json).await?,
    }
    Ok(())
}

async fn dispatch_stock(
    service: &CoreAdapter,
    command: StockCommand,
    json: bool,
) -> Result<(), ServiceError> {
    match command {
        StockCommand::Add(args) => print!(
            "{}",
            format::mutation(
                service
                    .add_stock(&args.item, args.quantity, &args.location)
                    .await?,
                json,
            )
        ),
        StockCommand::Adjust(args) => print!(
            "{}",
            format::mutation(
                service
                    .adjust_stock(&args.item, args.delta, &args.location)
                    .await?,
                json,
            )
        ),
        StockCommand::Move(args) => print!(
            "{}",
            format::mutation(
                service
                    .move_stock(&args.item, args.quantity, &args.from, &args.to)
                    .await?,
                json,
            )
        ),
    }
    Ok(())
}

fn report_error(error: ServiceError, json: bool) -> ExitCode {
    print_error(&error, json);
    ExitCode::from(1)
}

fn print_error(error: &ServiceError, json: bool) {
    if json {
        eprint!(
            "{}",
            format::json_output(&serde_json::json!({ "error": error.to_string() }))
        );
    } else {
        eprintln!("error: {error}");
    }
}
