# Stuff

Stuff is a desktop-first inventory tool for physical storage. It lets you
describe rooms, racks, shelves, boxes, bins, drawers, and organizers, then
track which items are stored in each place and how many you have.

## Goal

The goal is simple: when you need a part, you should be able to search for it
and see its exact location and quantity. The layout and inventory should match
the real storage space closely enough that putting something away or finding it
does not require a second system or guesswork.

## What you can do

- Build a tree of physical storage locations and place them in rooms or inside
  other storage.
- Use cell-less organizers as one undivided storage area, or draw compartment
  layouts as reusable typed organizers.
- Store items directly in rooms, racks, and shelves with physical dimensions
  and draggable workspace positions.
- Define rack types with shelf levels, then create physical racks from them.
- Add items with names, part numbers, categories, tags, and notes.
- Search for an item, inspect everything stored in a location, and list the
  storage tree.
- Add, remove, and move stock between locations.
- Find empty or partly filled locations, including locations that are reserved
  but currently empty.
- Use the desktop app for spatial editing or the CLI for quick commands and
  scripts.

## Project layout

The repository is a Rust workspace with three crates:

- `stuff-core` contains the storage model, inventory rules, validation, search,
  PostgreSQL access, and the demo data.
- `stuff-cli` provides the `stuff` command. It uses `stuff-core`, so it follows
  the same rules as the desktop app.
- `stuff-app` is a Tauri desktop app. The React frontend has Inventory, Layout,
  and Types tabs; its Rust commands call `stuff-core` instead of keeping a
  separate copy of the rules.

## Requirements

- Rust with the checked-in stable toolchain
- PostgreSQL
- Node.js and npm for the desktop app
- The normal Tauri system packages for your platform. On Linux, the release
  build uses WebKitGTK, AppIndicator, librsvg, `patchelf`, and `xdg-utils`.

The database must already exist and be reachable through a PostgreSQL URL. The
application creates and updates its tables through the embedded migration.

## Build and test

From the repository root:

```bash
cargo fmt --all -- --check
cargo build --workspace
cargo test --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings

npm --prefix stuff-app/ui ci
npm --prefix stuff-app/ui test
npm --prefix stuff-app/ui run build
```

## Use the CLI

Set the database URL before running the CLI:

```bash
export STUFF_DATABASE_URL='postgres://stuff:password@localhost/stuff'
```

Apply the database migration explicitly with:

```bash
cargo run -p stuff-cli --bin stuff -- init
```

The main commands are:

```bash
# Search by item name, part number, category, tag, or UUID.
cargo run -p stuff-cli --bin stuff -- find mcp6022

# Print all storage, or the part below one location.
cargo run -p stuff-cli --bin stuff -- tree
cargo run -p stuff-cli --bin stuff -- tree 'ICs Organizer'

# Show stock directly inside a location.
cargo run -p stuff-cli --bin stuff -- contents 'ICs Organizer'

# Find locations by estimated fill level or reservation state.
cargo run -p stuff-cli --bin stuff -- free --max-fill 25
cargo run -p stuff-cli --bin stuff -- free --completely-free

# Change stock.
cargo run -p stuff-cli --bin stuff -- stock add mcp6022 --quantity 12 --at 'ICs Organizer / 00'
cargo run -p stuff-cli --bin stuff -- stock adjust mcp6022 --by -2 --at 'ICs Organizer / 00'
cargo run -p stuff-cli --bin stuff -- stock move mcp6022 --quantity 5 \
  --from 'ICs Organizer / 00' --to 'Bench Box'
```

An item or location can usually be named by its name, path, or UUID. If a
selector matches more than one record, the command reports the ambiguity
instead of choosing one silently.

Use `--json` for machine-readable output:

```bash
cargo run -p stuff-cli --bin stuff --json find mcp6022
```

Run the CLI without a subcommand, or use `repl`, to open the interactive
prompt. It accepts the same read and stock commands. Type `help`, `help stock`,
`exit`, or `quit` at the prompt.

```bash
cargo run -p stuff-cli --bin stuff
```

Run `cargo run -p stuff-cli --bin stuff -- --help` for the complete command
list and option details.

## Use the desktop app

Install the frontend dependencies once:

```bash
npm --prefix stuff-app/ui ci
```

With `STUFF_DATABASE_URL` set, start the app with:

```bash
npm --prefix stuff-app/ui run tauri:dev
```

The checked-in helper uses
`postgres://root:root@127.0.0.1:5432/stuff` when no URL is set:

```bash
./run-local.sh
```

`run-local.sh` starts the Tauri development app. It does not create or start
PostgreSQL, so make sure the database at that URL is running first. You can
override it as usual:

```bash
STUFF_DATABASE_URL='postgres://stuff:password@localhost/stuff' ./run-local.sh
```

If the desktop app has no database URL, or cannot connect to the database, it
opens demo data. The demo is clearly marked in the window; edits there stay in
memory and are not saved to PostgreSQL.

## Typical workflow

1. Open the Layout tab and create the rooms and storage objects that exist in
   the real space.
2. In Types, create reusable organizer or rack types when several physical
   objects share the same shape. Create instances from those types and place
   them in the layout.
3. In Inventory, add each item and its starting quantity at a storage location
   or organizer compartment.
4. Search when you need something. Adjust quantities when stock changes, and
   move stock when it changes location.

The UI uses centimetres for dimensions. The core stores integer millimetres so
placements and sizes do not accumulate floating-point rounding errors.

## How it works

`stuff-core` keeps the current storage layout, item list, stock, reservations,
and fill estimates in one in-memory snapshot. In live mode, that snapshot is
loaded from PostgreSQL and every successful change is written back in a
transaction.

Each saved change increments a revision number. Before writing, the service
checks that the revision it loaded is still current. If another client saved
first, the second write returns a conflict instead of overwriting newer data.

After a successful write, PostgreSQL sends a small `LISTEN`/`NOTIFY` message
containing the change topic and revision. Other desktop clients receive the
message and fetch the current data again. The notification is only a signal;
the full inventory is always read from PostgreSQL.

The CLI connects directly to `stuff-core`. The desktop app connects through
its Tauri Rust commands, and the React UI receives the same current snapshot.
That keeps search, placement checks, stock changes, and conflict handling the
same in both interfaces.
