//! Load normalized L2 OCB files into application-owned structs.
//!
//! This example is the read-side companion to `l2_parquet_to_ocb`: it opens the
//! order/trade and market-data OCB files with the public safe wrapper, projects
//! only the columns needed by a replay/LOB loader, applies row-group predicates,
//! and copies returned batches into ordinary Rust structs.
//!
//! Example:
//!
//! ```text
//! cargo run -p arcadia-tio-rs --features format-ocb \
//!   --example l2_ocb_load -- \
//!   --input-dir target/l2-parquet-ocb-example \
//!   --max-rows 20
//! ```
//!
//! The public OCB API currently returns owned batches for selected row groups.
//! Use `--day-key`, `--channel`, and `--symbol-code` predicates to keep reads
//! bounded when working with large shards.

use std::env;
use std::error::Error;
use std::fmt;
use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf};

use arcadia_tio_rs::ocb::{
    ColumnArray, ColumnBatch, ColumnBundleFile, PredicateValue, PrimitiveValues, Projection,
    ReadReport, ReadRequest, RowGroupPredicate,
};

const DEFAULT_MAX_ROWS: usize = 20;

const ORDER_TRADE_PROJECTION: &[&str] = &[
    "day_key",
    "partition_key",
    "order_key",
    "tie_breaker",
    "event_kind_code",
    "symbol_code",
    "venue_code",
    "price_scaled",
    "volume",
    "id_0",
    "id_1",
    "side_code",
    "type_code",
    "receive_nano",
    "exchange_time",
];

const MARKET_PREFIX_PROJECTION: &[&str] = &[
    "day_key",
    "symbol_code",
    "venue_code",
    "receive_nano",
    "trade_time",
    "tie_breaker",
    "last_price_scaled",
    "total_trade_number",
    "total_trade_volume",
    "total_trade_amount_scaled",
    "weighted_avg_ask_price_scaled",
    "weighted_avg_bid_price_scaled",
];

type AppResult<T> = Result<T, Box<dyn Error>>;

#[derive(Debug)]
struct Args {
    order_trade: Option<PathBuf>,
    market_data: Option<PathBuf>,
    max_rows: Option<usize>,
    day_key: Option<i32>,
    channel: Option<i64>,
    symbol_code: Option<i32>,
    read_threads: usize,
    load_order_trade: bool,
    load_market_data: bool,
}

#[derive(Debug, Clone)]
struct LobEvent {
    day_key: i32,
    channel: i64,
    sequence: i64,
    tie_breaker: i64,
    kind: EventKind,
    symbol_code: i32,
    venue: Venue,
    price_cents: i64,
    volume: i64,
    id_0: i64,
    id_1: i64,
    side: Side,
    raw_type_code: i32,
    receive_nano: i64,
    exchange_time: i64,
}

#[derive(Debug, Clone)]
struct MarketSnapshot {
    day_key: i32,
    symbol_code: i32,
    venue: Venue,
    receive_nano: i64,
    trade_time: i64,
    tie_breaker: i64,
    last_price_cents: i64,
    total_trade_number: i64,
    total_trade_volume: i64,
    total_trade_amount_cents: i64,
    weighted_avg_ask_price_cents: i64,
    weighted_avg_bid_price_cents: i64,
    levels: [BookLevel; 10],
}

#[derive(Debug, Clone, Copy, Default)]
struct BookLevel {
    ask_price_cents: i64,
    ask_volume: i64,
    ask_count: i64,
    bid_price_cents: i64,
    bid_volume: i64,
    bid_count: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EventKind {
    Unknown,
    AddOrder,
    CancelOrder,
    ExecuteTrade,
    SequencePlaceholder,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Side {
    Unknown,
    Buy,
    Sell,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Venue {
    Unknown,
    Xshg,
    Xshe,
}

fn main() -> AppResult<()> {
    let args = Args::parse()?;

    if args.load_order_trade {
        let path = args
            .order_trade
            .as_deref()
            .ok_or_else(|| invalid("missing --order-trade or --input-dir"))?;
        let (events, report) = load_lob_events(path, &args)?;
        print_report("order+trade", &report);
        println!("loaded {} order/trade event(s)", events.len());
        for event in events.iter().take(3) {
            println!("  {event}");
        }
    }

    if args.load_market_data {
        let path = args
            .market_data
            .as_deref()
            .ok_or_else(|| invalid("missing --market-data or --input-dir"))?;
        let (snapshots, report) = load_market_snapshots(path, &args)?;
        print_report("market-data", &report);
        println!("loaded {} market-data snapshot(s)", snapshots.len());
        for snapshot in snapshots.iter().take(3) {
            println!("  {snapshot}");
        }
    }

    Ok(())
}

impl Args {
    fn parse() -> AppResult<Self> {
        let mut input_dir = None;
        let mut order_trade = None;
        let mut market_data = None;
        let mut max_rows = Some(DEFAULT_MAX_ROWS);
        let mut day_key = None;
        let mut channel = None;
        let mut symbol_code = None;
        let mut read_threads = 1usize;
        let mut only = "both".to_string();

        let mut args = env::args().skip(1);
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--input-dir" => input_dir = Some(PathBuf::from(next_value(&mut args, &arg)?)),
                "--order-trade" => {
                    order_trade = Some(PathBuf::from(next_value(&mut args, &arg)?));
                }
                "--market-data" | "--market" => {
                    market_data = Some(PathBuf::from(next_value(&mut args, &arg)?));
                }
                "--max-rows" => max_rows = Some(parse_value(&mut args, &arg)?),
                "--all-rows" => max_rows = None,
                "--day-key" => day_key = Some(parse_value(&mut args, &arg)?),
                "--channel" => channel = Some(parse_value(&mut args, &arg)?),
                "--symbol-code" => symbol_code = Some(parse_value(&mut args, &arg)?),
                "--read-threads" => read_threads = parse_value(&mut args, &arg)?,
                "--only" => only = next_value(&mut args, &arg)?,
                "--help" | "-h" => {
                    print_usage();
                    std::process::exit(0);
                }
                other => return Err(invalid(format!("unknown argument: {other}"))),
            }
        }

        if read_threads == 0 {
            return Err(invalid("--read-threads must be greater than zero"));
        }
        if let Some(0) = max_rows {
            return Err(invalid("--max-rows must be greater than zero"));
        }
        let load_order_trade = matches!(only.as_str(), "both" | "order-trade" | "orders");
        let load_market_data = matches!(only.as_str(), "both" | "market-data" | "market");
        if !load_order_trade && !load_market_data {
            return Err(invalid(
                "--only must be one of: both, order-trade, market-data",
            ));
        }

        if let Some(input_dir) = input_dir {
            order_trade.get_or_insert_with(|| input_dir.join("l2-order-trade.ocb"));
            market_data.get_or_insert_with(|| input_dir.join("l2-market-data.ocb"));
        }

        Ok(Self {
            order_trade: if load_order_trade { order_trade } else { None },
            market_data: if load_market_data { market_data } else { None },
            max_rows,
            day_key,
            channel,
            symbol_code,
            read_threads,
            load_order_trade,
            load_market_data,
        })
    }
}

fn load_lob_events(path: &Path, args: &Args) -> AppResult<(Vec<LobEvent>, ReadReport)> {
    let file = ColumnBundleFile::open(path)?;
    let metadata = file.metadata()?;
    println!(
        "opened order+trade OCB: path={} rows={} row_groups={} columns={}",
        path.display(),
        metadata.row_count,
        metadata.row_group_count,
        metadata.columns.len()
    );

    let predicates = order_trade_predicates(args);
    let outcome = file.read_batches(&ReadRequest {
        projection: Projection::Names(names(ORDER_TRADE_PROJECTION)),
        predicates,
        max_threads: args.read_threads,
        ..ReadRequest::default()
    })?;

    let mut events = Vec::new();
    for batch in &outcome.batches {
        for row in 0..batch_len(batch)? {
            if args.max_rows.is_some_and(|limit| events.len() >= limit) {
                return Ok((events, outcome.report));
            }
            events.push(lob_event_from_batch(batch, row)?);
        }
    }
    Ok((events, outcome.report))
}

fn load_market_snapshots(path: &Path, args: &Args) -> AppResult<(Vec<MarketSnapshot>, ReadReport)> {
    let file = ColumnBundleFile::open(path)?;
    let metadata = file.metadata()?;
    println!(
        "opened market-data OCB: path={} rows={} row_groups={} columns={}",
        path.display(),
        metadata.row_count,
        metadata.row_group_count,
        metadata.columns.len()
    );

    let predicates = market_data_predicates(args);
    let outcome = file.read_batches(&ReadRequest {
        projection: Projection::Names(market_projection_names()),
        predicates,
        max_threads: args.read_threads,
        ..ReadRequest::default()
    })?;

    let mut snapshots = Vec::new();
    for batch in &outcome.batches {
        for row in 0..batch_len(batch)? {
            if args.max_rows.is_some_and(|limit| snapshots.len() >= limit) {
                return Ok((snapshots, outcome.report));
            }
            snapshots.push(market_snapshot_from_batch(batch, row)?);
        }
    }
    Ok((snapshots, outcome.report))
}

fn lob_event_from_batch(batch: &ColumnBatch, row: usize) -> AppResult<LobEvent> {
    Ok(LobEvent {
        day_key: i32_value(batch, "day_key", row)?,
        channel: i64_value(batch, "partition_key", row)?,
        sequence: i64_value(batch, "order_key", row)?,
        tie_breaker: i64_value(batch, "tie_breaker", row)?,
        kind: event_kind(i32_value(batch, "event_kind_code", row)?),
        symbol_code: i32_value(batch, "symbol_code", row)?,
        venue: venue(i32_value(batch, "venue_code", row)?),
        price_cents: i64_value(batch, "price_scaled", row)?,
        volume: i64_value(batch, "volume", row)?,
        id_0: i64_value(batch, "id_0", row)?,
        id_1: i64_value(batch, "id_1", row)?,
        side: side(i32_value(batch, "side_code", row)?),
        raw_type_code: i32_value(batch, "type_code", row)?,
        receive_nano: i64_value(batch, "receive_nano", row)?,
        exchange_time: i64_value(batch, "exchange_time", row)?,
    })
}

fn market_snapshot_from_batch(batch: &ColumnBatch, row: usize) -> AppResult<MarketSnapshot> {
    let mut levels = [BookLevel::default(); 10];
    for level in 1..=10 {
        let idx = level - 1;
        levels[idx] = BookLevel {
            ask_price_cents: i64_value(batch, &format!("ask_price_scaled_{level}"), row)?,
            ask_volume: i64_value(batch, &format!("ask_volume_{level}"), row)?,
            ask_count: i64_value(batch, &format!("ask_number_{level}"), row)?,
            bid_price_cents: i64_value(batch, &format!("bid_price_scaled_{level}"), row)?,
            bid_volume: i64_value(batch, &format!("bid_volume_{level}"), row)?,
            bid_count: i64_value(batch, &format!("bid_number_{level}"), row)?,
        };
    }

    Ok(MarketSnapshot {
        day_key: i32_value(batch, "day_key", row)?,
        symbol_code: i32_value(batch, "symbol_code", row)?,
        venue: venue(i32_value(batch, "venue_code", row)?),
        receive_nano: i64_value(batch, "receive_nano", row)?,
        trade_time: i64_value(batch, "trade_time", row)?,
        tie_breaker: i64_value(batch, "tie_breaker", row)?,
        last_price_cents: i64_value(batch, "last_price_scaled", row)?,
        total_trade_number: i64_value(batch, "total_trade_number", row)?,
        total_trade_volume: i64_value(batch, "total_trade_volume", row)?,
        total_trade_amount_cents: i64_value(batch, "total_trade_amount_scaled", row)?,
        weighted_avg_ask_price_cents: i64_value(batch, "weighted_avg_ask_price_scaled", row)?,
        weighted_avg_bid_price_cents: i64_value(batch, "weighted_avg_bid_price_scaled", row)?,
        levels,
    })
}

fn order_trade_predicates(args: &Args) -> Vec<RowGroupPredicate> {
    let mut predicates = Vec::new();
    if let Some(day_key) = args.day_key {
        predicates.push(eq_i32("day_key", day_key));
    }
    if let Some(channel) = args.channel {
        predicates.push(eq_i64("partition_key", channel));
    }
    predicates
}

fn market_data_predicates(args: &Args) -> Vec<RowGroupPredicate> {
    let mut predicates = Vec::new();
    if let Some(day_key) = args.day_key {
        predicates.push(eq_i32("day_key", day_key));
    }
    if let Some(symbol_code) = args.symbol_code {
        predicates.push(eq_i32("symbol_code", symbol_code));
    }
    predicates
}

fn eq_i32(column: &str, value: i32) -> RowGroupPredicate {
    RowGroupPredicate {
        column: column.to_string(),
        lower: Some(PredicateValue::I32(value)),
        upper: Some(PredicateValue::I32(value)),
    }
}

fn eq_i64(column: &str, value: i64) -> RowGroupPredicate {
    RowGroupPredicate {
        column: column.to_string(),
        lower: Some(PredicateValue::I64(value)),
        upper: Some(PredicateValue::I64(value)),
    }
}

fn batch_len(batch: &ColumnBatch) -> AppResult<usize> {
    usize::try_from(batch.row_count).map_err(|_| invalid("batch row count exceeds usize"))
}

fn column<'a>(batch: &'a ColumnBatch, name: &str) -> AppResult<&'a ColumnArray> {
    batch
        .columns
        .iter()
        .find(|column| column.name == name)
        .ok_or_else(|| invalid(format!("read batch did not include column {name}")))
}

fn i32_value(batch: &ColumnBatch, name: &str, row: usize) -> AppResult<i32> {
    let column = column(batch, name)?;
    match &column.values {
        PrimitiveValues::I32(values) => values
            .get(row)
            .copied()
            .ok_or_else(|| invalid(format!("column {name} row {row} out of bounds"))),
        PrimitiveValues::I64(values) => {
            let value = values
                .get(row)
                .copied()
                .ok_or_else(|| invalid(format!("column {name} row {row} out of bounds")))?;
            i32::try_from(value).map_err(|_| invalid(format!("column {name} value exceeds i32")))
        }
        other => Err(invalid(format!(
            "column {name} has non-integer values: {other:?}"
        ))),
    }
}

fn i64_value(batch: &ColumnBatch, name: &str, row: usize) -> AppResult<i64> {
    let column = column(batch, name)?;
    match &column.values {
        PrimitiveValues::I64(values) => values
            .get(row)
            .copied()
            .ok_or_else(|| invalid(format!("column {name} row {row} out of bounds"))),
        PrimitiveValues::I32(values) => values
            .get(row)
            .map(|value| i64::from(*value))
            .ok_or_else(|| invalid(format!("column {name} row {row} out of bounds"))),
        other => Err(invalid(format!(
            "column {name} has non-integer values: {other:?}"
        ))),
    }
}

fn names(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_string()).collect()
}

fn market_projection_names() -> Vec<String> {
    let mut names = names(MARKET_PREFIX_PROJECTION);
    for side in ["ask", "bid"] {
        for metric in ["price_scaled", "volume", "number"] {
            for level in 1..=10 {
                names.push(format!("{side}_{metric}_{level}"));
            }
        }
    }
    names
}

fn event_kind(code: i32) -> EventKind {
    match code {
        1 => EventKind::AddOrder,
        2 => EventKind::CancelOrder,
        3 => EventKind::ExecuteTrade,
        4 => EventKind::SequencePlaceholder,
        _ => EventKind::Unknown,
    }
}

fn side(code: i32) -> Side {
    match code {
        1 => Side::Buy,
        2 => Side::Sell,
        _ => Side::Unknown,
    }
}

fn venue(code: i32) -> Venue {
    match code {
        1 => Venue::Xshg,
        2 => Venue::Xshe,
        _ => Venue::Unknown,
    }
}

fn print_report(label: &str, report: &ReadReport) {
    println!(
        "{label} read report: requested_threads={} effective_threads={} selected_row_groups={} pruned_row_groups={} selected_column_chunks={} fallback={}",
        report.requested_threads,
        report.effective_threads,
        report.selected_row_groups,
        report.pruned_row_groups,
        report.selected_column_chunks,
        report.fallback_reason.as_deref().unwrap_or("none")
    );
}

impl fmt::Display for LobEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "day={} channel={} sequence={} tie={} kind={:?} symbol={} venue={:?} side={:?} price_cents={} volume={} id_0={} id_1={} type_code={} receive_nano={} exchange_time={}",
            self.day_key,
            self.channel,
            self.sequence,
            self.tie_breaker,
            self.kind,
            self.symbol_code,
            self.venue,
            self.side,
            self.price_cents,
            self.volume,
            self.id_0,
            self.id_1,
            self.raw_type_code,
            self.receive_nano,
            self.exchange_time
        )
    }
}

impl fmt::Display for MarketSnapshot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let top = self.levels[0];
        write!(
            f,
            "day={} symbol={} venue={:?} receive_nano={} trade_time={} tie={} last_price_cents={} total_trades={} total_volume={} total_amount_cents={} wav_ask_cents={} wav_bid_cents={} top_bid=({}, vol={}, count={}) top_ask=({}, vol={}, count={})",
            self.day_key,
            self.symbol_code,
            self.venue,
            self.receive_nano,
            self.trade_time,
            self.tie_breaker,
            self.last_price_cents,
            self.total_trade_number,
            self.total_trade_volume,
            self.total_trade_amount_cents,
            self.weighted_avg_ask_price_cents,
            self.weighted_avg_bid_price_cents,
            top.bid_price_cents,
            top.bid_volume,
            top.bid_count,
            top.ask_price_cents,
            top.ask_volume,
            top.ask_count
        )
    }
}

fn parse_value<T>(args: &mut impl Iterator<Item = String>, flag: &str) -> AppResult<T>
where
    T: std::str::FromStr,
    T::Err: fmt::Display,
{
    let raw = next_value(args, flag)?;
    raw.parse::<T>()
        .map_err(|source| invalid(format!("invalid value for {flag}: {source}")))
}

fn next_value(args: &mut impl Iterator<Item = String>, flag: &str) -> AppResult<String> {
    args.next()
        .ok_or_else(|| invalid(format!("missing value for {flag}")))
}

fn print_usage() {
    eprintln!(
        "usage: l2_ocb_load --input-dir DIR [--max-rows N|--all-rows] [--day-key YYYYMMDD] [--channel N] [--symbol-code N]\n\
         \n\
         optional explicit paths:\n\
           --order-trade l2-order-trade.ocb --market-data l2-market-data.ocb\n\
         \n\
         --only both|order-trade|market-data defaults to both. The default max rows copied into\n\
         example structs is {DEFAULT_MAX_ROWS}; use row-group predicates for large shards."
    );
}

fn invalid(message: impl Into<String>) -> Box<dyn Error> {
    Box::new(io::Error::new(ErrorKind::InvalidInput, message.into()))
}
