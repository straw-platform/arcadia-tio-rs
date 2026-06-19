//! Bounded L2 order/trade and market-data Parquet -> OCB conversion example.
//!
//! This example demonstrates how an application can use the public Rust OCB
//! wrapper to convert one schema-compatible L2 day into two generic OCB files:
//!
//! - normalized order+trade events from `L2ORDER.journal` + `L2TRADE.journal`;
//! - market-data snapshots from `L2MD.journal`.
//!
//! It intentionally uses the safe public `arcadia_tio_rs::ocb` API only. It is
//! a schema and integration example, not the production external-sort pipeline:
//! rows are materialized in memory before one OCB create call. Keep `--row-limit`
//! small for local smoke tests unless the caller has budgeted memory.
//!
//! Example:
//!
//! ```text
//! cargo run -p arcadia-tio-rs --features format-ocb,parquet \
//!   --example l2_parquet_to_ocb -- \
//!   --day-dir /path/to/l2_parquet/YYYYMMDD \
//!   --output-dir target/l2-parquet-ocb-example \
//!   --row-limit 10000 \
//!   --overwrite
//! ```

use std::env;
use std::error::Error;
use std::fs::{self, File};
use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf};

use arcadia_tio_rs::ocb::{
    self, ColumnBundleFile, LogicalKind, NullOrder, OrderingDirection, PhysicalType,
    PrimitiveValues, WriteColumn, WriteColumnChunk, WriteOrderingKey, WriteRowGroup, WriteSpec,
};
use parquet::file::reader::{FileReader, SerializedFileReader};
use parquet::record::{Field, Row};

const DEFAULT_ROW_LIMIT: usize = 10_000;
const DEFAULT_ROW_GROUP_SIZE: usize = 65_536;

const ORDER_TYPE_DELETE: u8 = b'D';
const ORDER_TYPE_STATE: u8 = b'S';
const ORDER_TYPE_ANY_PRICE: u8 = b'1';
const ORDER_TYPE_LIMIT_PRICE: u8 = b'2';
const ORDER_TYPE_BEST_PRICE: u8 = b'3';
const ORDER_TYPE_ADD: u8 = b'A';
const TRADE_TYPE_CANCEL: u8 = b'1';
const TRADE_TYPE_TRADE: u8 = b'2';

const ORDER_SIDE_BUY: u8 = b'0';
const ORDER_SIDE_SELL: u8 = b'1';
const TRADE_SIDE_UNKNOWN: u8 = b'0';
const TRADE_SIDE_BUY: u8 = b'1';
const TRADE_SIDE_SELL: u8 = b'2';

const EXCHANGE_XSHG: i64 = 1;
const EXCHANGE_XSHE: i64 = 2;

const ORDER_TRADE_COLUMNS: &[&str] = &[
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

const MARKET_PREFIX_COLUMNS: &[&str] = &[
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
    orders: PathBuf,
    trades: PathBuf,
    market_data: PathBuf,
    order_trade_output: PathBuf,
    market_data_output: PathBuf,
    row_limit: Option<usize>,
    row_group_size: usize,
    overwrite: bool,
    convert_order_trade: bool,
    convert_market_data: bool,
}

#[derive(Debug, Clone)]
struct OrderTradeRow {
    day_key: i32,
    partition_key: i64,
    order_key: i64,
    tie_breaker: i64,
    event_kind_code: i32,
    symbol_code: i32,
    venue_code: i32,
    price_scaled: i64,
    volume: i64,
    id_0: i64,
    id_1: i64,
    side_code: i32,
    type_code: i32,
    receive_nano: i64,
    exchange_time: i64,
}

#[derive(Debug, Clone)]
struct MarketDataRow {
    day_key: i32,
    symbol_code: i32,
    venue_code: i32,
    receive_nano: i64,
    trade_time: i64,
    tie_breaker: i64,
    last_price_scaled: i64,
    total_trade_number: i64,
    total_trade_volume: i64,
    total_trade_amount_scaled: i64,
    weighted_avg_ask_price_scaled: i64,
    weighted_avg_bid_price_scaled: i64,
    ask_price_scaled: [i64; 10],
    ask_volume: [i64; 10],
    ask_number: [i64; 10],
    bid_price_scaled: [i64; 10],
    bid_volume: [i64; 10],
    bid_number: [i64; 10],
}

#[derive(Debug, Clone)]
enum ColumnValues {
    I32(Vec<i32>),
    I64(Vec<i64>),
}

#[derive(Debug, Clone)]
struct ExampleColumn {
    name: String,
    logical_kind: LogicalKind,
    values: ColumnValues,
}

impl Args {
    fn parse() -> AppResult<Self> {
        let mut day_dir = None;
        let mut orders = None;
        let mut trades = None;
        let mut market_data = None;
        let mut output_dir = None;
        let mut order_trade_output = None;
        let mut market_data_output = None;
        let mut row_limit = Some(DEFAULT_ROW_LIMIT);
        let mut row_group_size = DEFAULT_ROW_GROUP_SIZE;
        let mut overwrite = false;
        let mut only = "both".to_string();

        let mut args = env::args().skip(1);
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--day-dir" => day_dir = Some(PathBuf::from(next_value(&mut args, &arg)?)),
                "--orders" => orders = Some(PathBuf::from(next_value(&mut args, &arg)?)),
                "--trades" => trades = Some(PathBuf::from(next_value(&mut args, &arg)?)),
                "--market-data" | "--market" | "--l2md" => {
                    market_data = Some(PathBuf::from(next_value(&mut args, &arg)?));
                }
                "--output-dir" => output_dir = Some(PathBuf::from(next_value(&mut args, &arg)?)),
                "--order-trade-output" => {
                    order_trade_output = Some(PathBuf::from(next_value(&mut args, &arg)?));
                }
                "--market-data-output" => {
                    market_data_output = Some(PathBuf::from(next_value(&mut args, &arg)?));
                }
                "--row-limit" => row_limit = Some(parse_value(&mut args, &arg)?),
                "--all-rows" => row_limit = None,
                "--row-group-size" => row_group_size = parse_value(&mut args, &arg)?,
                "--overwrite" => overwrite = true,
                "--only" => only = next_value(&mut args, &arg)?,
                "--help" | "-h" => {
                    print_usage();
                    std::process::exit(0);
                }
                other => return Err(invalid(format!("unknown argument: {other}"))),
            }
        }

        if row_group_size == 0 {
            return Err(invalid("--row-group-size must be greater than zero"));
        }
        if let Some(0) = row_limit {
            return Err(invalid("--row-limit must be greater than zero"));
        }
        let convert_order_trade = matches!(only.as_str(), "both" | "order-trade" | "orders");
        let convert_market_data = matches!(only.as_str(), "both" | "market-data" | "market");
        if !convert_order_trade && !convert_market_data {
            return Err(invalid(
                "--only must be one of: both, order-trade, market-data",
            ));
        }

        if let Some(day_dir) = day_dir {
            orders.get_or_insert_with(|| day_dir.join("L2ORDER.journal"));
            trades.get_or_insert_with(|| day_dir.join("L2TRADE.journal"));
            market_data.get_or_insert_with(|| day_dir.join("L2MD.journal"));
        }

        let output_dir =
            output_dir.unwrap_or_else(|| PathBuf::from("target/l2-parquet-ocb-example"));
        let order_trade_output =
            order_trade_output.unwrap_or_else(|| output_dir.join("l2-order-trade.ocb"));
        let market_data_output =
            market_data_output.unwrap_or_else(|| output_dir.join("l2-market-data.ocb"));

        let orders = if convert_order_trade {
            orders.ok_or_else(|| invalid("missing --orders or --day-dir"))?
        } else {
            PathBuf::new()
        };
        let trades = if convert_order_trade {
            trades.ok_or_else(|| invalid("missing --trades or --day-dir"))?
        } else {
            PathBuf::new()
        };
        let market_data = if convert_market_data {
            market_data.ok_or_else(|| invalid("missing --market-data or --day-dir"))?
        } else {
            PathBuf::new()
        };

        Ok(Self {
            orders,
            trades,
            market_data,
            order_trade_output,
            market_data_output,
            row_limit,
            row_group_size,
            overwrite,
            convert_order_trade,
            convert_market_data,
        })
    }
}

fn main() -> AppResult<()> {
    let args = Args::parse()?;
    if let Some(parent) = args.order_trade_output.parent() {
        fs::create_dir_all(parent)?;
    }
    if let Some(parent) = args.market_data_output.parent() {
        fs::create_dir_all(parent)?;
    }

    if args.convert_order_trade {
        convert_order_trade(&args)?;
    }
    if args.convert_market_data {
        convert_market_data(&args)?;
    }
    Ok(())
}

fn convert_order_trade(args: &Args) -> AppResult<()> {
    prepare_output(&args.order_trade_output, args.overwrite)?;
    let mut rows = Vec::new();
    let order_count = read_orders(&args.orders, args.row_limit, 0, &mut rows)?;
    let trade_count = read_trades(
        &args.trades,
        args.row_limit,
        i64::try_from(order_count).map_err(|_| invalid("order row count exceeds i64"))?,
        &mut rows,
    )?;
    rows.sort_unstable_by_key(|row| {
        (
            row.day_key,
            row.partition_key,
            row.order_key,
            row.tie_breaker,
        )
    });

    let spec = order_trade_spec(&rows, args.row_group_size)?;
    ocb::create(&args.order_trade_output, &spec)?;
    let metadata = ColumnBundleFile::open(&args.order_trade_output)?.metadata()?;
    println!(
        "wrote order+trade OCB: path={} orders={} trades={} rows={} row_groups={}",
        args.order_trade_output.display(),
        order_count,
        trade_count,
        metadata.row_count,
        metadata.row_group_count
    );
    Ok(())
}

fn convert_market_data(args: &Args) -> AppResult<()> {
    prepare_output(&args.market_data_output, args.overwrite)?;
    let mut rows = read_market_rows(&args.market_data, args.row_limit)?;
    rows.sort_unstable_by_key(|row| {
        (
            row.day_key,
            row.symbol_code,
            row.receive_nano,
            row.tie_breaker,
        )
    });

    let spec = market_data_spec(&rows, args.row_group_size)?;
    ocb::create(&args.market_data_output, &spec)?;
    let metadata = ColumnBundleFile::open(&args.market_data_output)?.metadata()?;
    println!(
        "wrote market-data OCB: path={} rows={} row_groups={}",
        args.market_data_output.display(),
        metadata.row_count,
        metadata.row_group_count
    );
    Ok(())
}

fn read_orders(
    path: &Path,
    row_limit: Option<usize>,
    source_ordinal_start: i64,
    out: &mut Vec<OrderTradeRow>,
) -> AppResult<usize> {
    let reader = parquet_reader(path)?;
    let mut rows_read = 0usize;
    for row in reader.get_row_iter(None)? {
        if row_limit.is_some_and(|limit| rows_read >= limit) {
            break;
        }
        let row = row?;
        let source_ordinal = checked_source_ordinal(source_ordinal_start, rows_read)?;
        out.push(order_row(&row, source_ordinal)?);
        rows_read += 1;
    }
    Ok(rows_read)
}

fn read_trades(
    path: &Path,
    row_limit: Option<usize>,
    source_ordinal_start: i64,
    out: &mut Vec<OrderTradeRow>,
) -> AppResult<usize> {
    let reader = parquet_reader(path)?;
    let mut rows_read = 0usize;
    for row in reader.get_row_iter(None)? {
        if row_limit.is_some_and(|limit| rows_read >= limit) {
            break;
        }
        let row = row?;
        let source_ordinal = checked_source_ordinal(source_ordinal_start, rows_read)?;
        out.push(trade_row(&row, source_ordinal)?);
        rows_read += 1;
    }
    Ok(rows_read)
}

fn read_market_rows(path: &Path, row_limit: Option<usize>) -> AppResult<Vec<MarketDataRow>> {
    let reader = parquet_reader(path)?;
    let mut rows = Vec::new();
    for row in reader.get_row_iter(None)? {
        if row_limit.is_some_and(|limit| rows.len() >= limit) {
            break;
        }
        let row = row?;
        let source_ordinal =
            i64::try_from(rows.len()).map_err(|_| invalid("row count exceeds i64"))?;
        rows.push(market_row(&row, source_ordinal)?);
    }
    Ok(rows)
}

fn order_row(row: &Row, source_ordinal: i64) -> AppResult<OrderTradeRow> {
    let exchange_time = i64_value(row, "OrderTime")?;
    let order_type = raw_u8(row, "OrderType")?;
    let bs_flag = raw_u8(row, "BSFlag")?;
    Ok(OrderTradeRow {
        day_key: day_key_from_raw_time(exchange_time),
        partition_key: i64_value(row, "ChannelNo")?,
        order_key: i64_value(row, "BizIndex")?,
        tie_breaker: source_ordinal,
        event_kind_code: order_event_kind(order_type),
        symbol_code: numeric_symbol_code(&string_value(row, "InstrumentCode")?)?,
        venue_code: venue_code(i64_value(row, "ExchangeID")?),
        price_scaled: scaled_price_cents(f64_value(row, "Price")?),
        volume: scaled_volume(f64_value(row, "Volume")?),
        id_0: i64_value(row, "OrderNo")?,
        id_1: i64_value(row, "OrderIndex")?,
        side_code: order_side_code(bs_flag),
        type_code: i32::from(order_type),
        receive_nano: i64_value(row, "nano")?,
        exchange_time,
    })
}

fn trade_row(row: &Row, source_ordinal: i64) -> AppResult<OrderTradeRow> {
    let exchange_time = i64_value(row, "TradeTime")?;
    let trade_type = raw_u8(row, "TradeType")?;
    let bs_flag = raw_u8(row, "BSFlag")?;
    Ok(OrderTradeRow {
        day_key: day_key_from_raw_time(exchange_time),
        partition_key: i64_value(row, "ChannelNo")?,
        order_key: i64_value(row, "BizIndex")?,
        tie_breaker: source_ordinal,
        event_kind_code: trade_event_kind(trade_type),
        symbol_code: numeric_symbol_code(&string_value(row, "InstrumentCode")?)?,
        venue_code: venue_code(i64_value(row, "ExchangeID")?),
        price_scaled: scaled_price_cents(f64_value(row, "Price")?),
        volume: scaled_volume(f64_value(row, "Volume")?),
        id_0: i64_value(row, "BidNo")?,
        id_1: i64_value(row, "AskNo")?,
        side_code: trade_side_code(bs_flag),
        type_code: i32::from(trade_type),
        receive_nano: i64_value(row, "nano")?,
        exchange_time,
    })
}

fn market_row(row: &Row, source_ordinal: i64) -> AppResult<MarketDataRow> {
    let trade_time = i64_value(row, "TradeTime")?;
    let mut ask_price_scaled = [0i64; 10];
    let mut ask_volume = [0i64; 10];
    let mut ask_number = [0i64; 10];
    let mut bid_price_scaled = [0i64; 10];
    let mut bid_volume = [0i64; 10];
    let mut bid_number = [0i64; 10];
    for level in 1..=10 {
        let idx = level - 1;
        ask_price_scaled[idx] = scaled_price_cents(f64_value(row, &format!("AskPrice{level}"))?);
        ask_volume[idx] = scaled_volume(f64_value(row, &format!("AskVolume{level}"))?);
        ask_number[idx] = i64_value(row, &format!("AskNumber{level}"))?;
        bid_price_scaled[idx] = scaled_price_cents(f64_value(row, &format!("BidPrice{level}"))?);
        bid_volume[idx] = scaled_volume(f64_value(row, &format!("BidVolume{level}"))?);
        bid_number[idx] = i64_value(row, &format!("BidNumber{level}"))?;
    }
    Ok(MarketDataRow {
        day_key: day_key_from_raw_time(trade_time),
        symbol_code: numeric_symbol_code(&string_value(row, "InstrumentCode")?)?,
        venue_code: venue_code(i64_value(row, "ExchangeID")?),
        receive_nano: i64_value(row, "nano")?,
        trade_time,
        tie_breaker: source_ordinal,
        last_price_scaled: scaled_price_cents(f64_value(row, "LastPrice")?),
        total_trade_number: scaled_volume(f64_value(row, "TotalTradeNumber")?),
        total_trade_volume: scaled_volume(f64_value(row, "TotalTradeVolume")?),
        total_trade_amount_scaled: scaled_price_cents(f64_value(row, "TotalTradeAmount")?),
        weighted_avg_ask_price_scaled: scaled_price_cents(f64_value(row, "WeightedAvgAskPrice")?),
        weighted_avg_bid_price_scaled: scaled_price_cents(f64_value(row, "WeightedAvgBidPrice")?),
        ask_price_scaled,
        ask_volume,
        ask_number,
        bid_price_scaled,
        bid_volume,
        bid_number,
    })
}

fn order_trade_spec(rows: &[OrderTradeRow], row_group_size: usize) -> AppResult<WriteSpec> {
    let columns = vec![
        column_i32(
            "day_key",
            LogicalKind::OpaqueKey,
            rows.iter().map(|r| r.day_key).collect(),
        ),
        column_i64(
            "partition_key",
            LogicalKind::OpaqueKey,
            rows.iter().map(|r| r.partition_key).collect(),
        ),
        column_i64(
            "order_key",
            LogicalKind::OpaqueKey,
            rows.iter().map(|r| r.order_key).collect(),
        ),
        column_i64(
            "tie_breaker",
            LogicalKind::OpaqueKey,
            rows.iter().map(|r| r.tie_breaker).collect(),
        ),
        column_i32(
            "event_kind_code",
            LogicalKind::EnumCode,
            rows.iter().map(|r| r.event_kind_code).collect(),
        ),
        column_i32(
            "symbol_code",
            LogicalKind::OpaqueKey,
            rows.iter().map(|r| r.symbol_code).collect(),
        ),
        column_i32(
            "venue_code",
            LogicalKind::EnumCode,
            rows.iter().map(|r| r.venue_code).collect(),
        ),
        column_i64(
            "price_scaled",
            LogicalKind::ScaledInteger,
            rows.iter().map(|r| r.price_scaled).collect(),
        ),
        column_i64(
            "volume",
            LogicalKind::Plain,
            rows.iter().map(|r| r.volume).collect(),
        ),
        column_i64(
            "id_0",
            LogicalKind::Plain,
            rows.iter().map(|r| r.id_0).collect(),
        ),
        column_i64(
            "id_1",
            LogicalKind::Plain,
            rows.iter().map(|r| r.id_1).collect(),
        ),
        column_i32(
            "side_code",
            LogicalKind::EnumCode,
            rows.iter().map(|r| r.side_code).collect(),
        ),
        column_i32(
            "type_code",
            LogicalKind::EnumCode,
            rows.iter().map(|r| r.type_code).collect(),
        ),
        column_i64(
            "receive_nano",
            LogicalKind::TimestampNanosLike,
            rows.iter().map(|r| r.receive_nano).collect(),
        ),
        column_i64(
            "exchange_time",
            LogicalKind::TimestampNanosLike,
            rows.iter().map(|r| r.exchange_time).collect(),
        ),
    ];
    write_spec_from_columns(columns, ORDER_TRADE_COLUMNS, &[0, 1, 2, 3], row_group_size)
}

fn market_data_spec(rows: &[MarketDataRow], row_group_size: usize) -> AppResult<WriteSpec> {
    let mut columns = vec![
        column_i32(
            "day_key",
            LogicalKind::OpaqueKey,
            rows.iter().map(|r| r.day_key).collect(),
        ),
        column_i32(
            "symbol_code",
            LogicalKind::OpaqueKey,
            rows.iter().map(|r| r.symbol_code).collect(),
        ),
        column_i32(
            "venue_code",
            LogicalKind::EnumCode,
            rows.iter().map(|r| r.venue_code).collect(),
        ),
        column_i64(
            "receive_nano",
            LogicalKind::TimestampNanosLike,
            rows.iter().map(|r| r.receive_nano).collect(),
        ),
        column_i64(
            "trade_time",
            LogicalKind::TimestampNanosLike,
            rows.iter().map(|r| r.trade_time).collect(),
        ),
        column_i64(
            "tie_breaker",
            LogicalKind::OpaqueKey,
            rows.iter().map(|r| r.tie_breaker).collect(),
        ),
        column_i64(
            "last_price_scaled",
            LogicalKind::ScaledInteger,
            rows.iter().map(|r| r.last_price_scaled).collect(),
        ),
        column_i64(
            "total_trade_number",
            LogicalKind::Plain,
            rows.iter().map(|r| r.total_trade_number).collect(),
        ),
        column_i64(
            "total_trade_volume",
            LogicalKind::Plain,
            rows.iter().map(|r| r.total_trade_volume).collect(),
        ),
        column_i64(
            "total_trade_amount_scaled",
            LogicalKind::ScaledInteger,
            rows.iter().map(|r| r.total_trade_amount_scaled).collect(),
        ),
        column_i64(
            "weighted_avg_ask_price_scaled",
            LogicalKind::ScaledInteger,
            rows.iter()
                .map(|r| r.weighted_avg_ask_price_scaled)
                .collect(),
        ),
        column_i64(
            "weighted_avg_bid_price_scaled",
            LogicalKind::ScaledInteger,
            rows.iter()
                .map(|r| r.weighted_avg_bid_price_scaled)
                .collect(),
        ),
    ];
    let mut names = MARKET_PREFIX_COLUMNS
        .iter()
        .map(|name| (*name).to_string())
        .collect::<Vec<_>>();

    for side in ["ask", "bid"] {
        for metric in ["price_scaled", "volume", "number"] {
            for level in 1..=10 {
                let name = format!("{side}_{metric}_{level}");
                names.push(name.clone());
                let values = rows
                    .iter()
                    .map(|row| {
                        let idx = level - 1;
                        match (side, metric) {
                            ("ask", "price_scaled") => row.ask_price_scaled[idx],
                            ("ask", "volume") => row.ask_volume[idx],
                            ("ask", "number") => row.ask_number[idx],
                            ("bid", "price_scaled") => row.bid_price_scaled[idx],
                            ("bid", "volume") => row.bid_volume[idx],
                            ("bid", "number") => row.bid_number[idx],
                            _ => unreachable!(),
                        }
                    })
                    .collect();
                let logical_kind = if metric == "price_scaled" {
                    LogicalKind::ScaledInteger
                } else {
                    LogicalKind::Plain
                };
                columns.push(column_i64(name, logical_kind, values));
            }
        }
    }

    let name_refs = names.iter().map(String::as_str).collect::<Vec<_>>();
    write_spec_from_columns(columns, &name_refs, &[0, 1, 3, 5], row_group_size)
}

fn write_spec_from_columns(
    columns: Vec<ExampleColumn>,
    expected_names: &[&str],
    ordering_ids: &[usize],
    row_group_size: usize,
) -> AppResult<WriteSpec> {
    if columns.is_empty() {
        return Err(invalid("cannot build OCB without columns"));
    }
    if columns.len() != expected_names.len() {
        return Err(invalid(format!(
            "column definition/name mismatch: {} definitions vs {} names",
            columns.len(),
            expected_names.len()
        )));
    }
    for (column, expected_name) in columns.iter().zip(expected_names.iter()) {
        if column.name != *expected_name {
            return Err(invalid(format!(
                "internal column order mismatch: got {}, expected {}",
                column.name, expected_name
            )));
        }
    }
    let row_count = columns[0].len();
    for column in &columns {
        if column.len() != row_count {
            return Err(invalid(format!(
                "column {} has {} rows, expected {row_count}",
                column.name,
                column.len()
            )));
        }
    }

    let write_columns = columns
        .iter()
        .map(|column| WriteColumn {
            name: column.name.clone(),
            physical_type: column.physical_type(),
            logical_kind: column.logical_kind,
            dictionary_id: None,
            scale: if matches!(column.logical_kind, LogicalKind::ScaledInteger) {
                -2
            } else {
                0
            },
            nullable: false,
        })
        .collect::<Vec<_>>();

    let mut row_groups = Vec::new();
    let mut start = 0usize;
    while start < row_count {
        let end = row_count.min(start + row_group_size);
        let chunks = columns
            .iter()
            .enumerate()
            .map(|(column_id, column)| WriteColumnChunk {
                column_id: u32::try_from(column_id).expect("example column id fits u32"),
                values: column.chunk_values(start, end),
                validity: None,
            })
            .collect();
        row_groups.push(WriteRowGroup { columns: chunks });
        start = end;
    }

    let ordering_keys = ordering_ids
        .iter()
        .map(|column_id| WriteOrderingKey {
            column_id: u32::try_from(*column_id).expect("example column id fits u32"),
            direction: OrderingDirection::Ascending,
            null_order: NullOrder::NoNulls,
        })
        .collect();

    Ok(WriteSpec {
        columns: write_columns,
        dictionaries: Vec::new(),
        row_groups,
        ordering_keys,
    })
}

impl ExampleColumn {
    fn len(&self) -> usize {
        match &self.values {
            ColumnValues::I32(values) => values.len(),
            ColumnValues::I64(values) => values.len(),
        }
    }

    fn physical_type(&self) -> PhysicalType {
        match &self.values {
            ColumnValues::I32(_) => PhysicalType::I32,
            ColumnValues::I64(_) => PhysicalType::I64,
        }
    }

    fn chunk_values(&self, start: usize, end: usize) -> PrimitiveValues {
        match &self.values {
            ColumnValues::I32(values) => PrimitiveValues::I32(values[start..end].to_vec()),
            ColumnValues::I64(values) => PrimitiveValues::I64(values[start..end].to_vec()),
        }
    }
}

fn column_i32(
    name: impl Into<String>,
    logical_kind: LogicalKind,
    values: Vec<i32>,
) -> ExampleColumn {
    ExampleColumn {
        name: name.into(),
        logical_kind,
        values: ColumnValues::I32(values),
    }
}

fn column_i64(
    name: impl Into<String>,
    logical_kind: LogicalKind,
    values: Vec<i64>,
) -> ExampleColumn {
    ExampleColumn {
        name: name.into(),
        logical_kind,
        values: ColumnValues::I64(values),
    }
}

fn prepare_output(path: &Path, overwrite: bool) -> AppResult<()> {
    if path.exists() {
        if overwrite {
            fs::remove_file(path)?;
        } else {
            return Err(invalid(format!(
                "output {} already exists; pass --overwrite to replace it",
                path.display()
            )));
        }
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    Ok(())
}

fn parquet_reader(path: &Path) -> AppResult<SerializedFileReader<File>> {
    let file = File::open(path).map_err(|source| path_io("open parquet", path, source))?;
    Ok(SerializedFileReader::new(file)?)
}

fn checked_source_ordinal(start: i64, rows_read: usize) -> AppResult<i64> {
    start
        .checked_add(i64::try_from(rows_read).map_err(|_| invalid("row index exceeds i64"))?)
        .ok_or_else(|| invalid("source ordinal overflow"))
}

fn field<'a>(row: &'a Row, name: &str) -> AppResult<&'a Field> {
    row.get_column_iter()
        .find(|(column, _)| column.as_str() == name)
        .map(|(_, field)| field)
        .ok_or_else(|| invalid(format!("missing parquet column {name}")))
}

fn i64_value(row: &Row, name: &str) -> AppResult<i64> {
    match field(row, name)? {
        Field::Byte(value) => Ok(i64::from(*value)),
        Field::Short(value) => Ok(i64::from(*value)),
        Field::Int(value) => Ok(i64::from(*value)),
        Field::Long(value) => Ok(*value),
        Field::UByte(value) => Ok(i64::from(*value)),
        Field::UShort(value) => Ok(i64::from(*value)),
        Field::UInt(value) => Ok(i64::from(*value)),
        Field::ULong(value) => {
            i64::try_from(*value).map_err(|_| invalid(format!("column {name} overflows i64")))
        }
        Field::Null => Err(invalid(format!("column {name} is null"))),
        other => Err(invalid(format!(
            "column {name} has non-integer field {other:?}"
        ))),
    }
}

fn f64_value(row: &Row, name: &str) -> AppResult<f64> {
    match field(row, name)? {
        Field::Float(value) => Ok(f64::from(*value)),
        Field::Double(value) => Ok(*value),
        Field::Byte(value) => Ok(f64::from(*value)),
        Field::Short(value) => Ok(f64::from(*value)),
        Field::Int(value) => Ok(f64::from(*value)),
        Field::Long(value) => Ok(*value as f64),
        Field::UByte(value) => Ok(f64::from(*value)),
        Field::UShort(value) => Ok(f64::from(*value)),
        Field::UInt(value) => Ok(f64::from(*value)),
        Field::ULong(value) => Ok(*value as f64),
        Field::Null => Ok(0.0),
        other => Err(invalid(format!(
            "column {name} has non-numeric field {other:?}"
        ))),
    }
}

fn string_value(row: &Row, name: &str) -> AppResult<String> {
    match field(row, name)? {
        Field::Str(value) => Ok(value.clone()),
        Field::Bytes(value) => Ok(value.as_utf8()?.to_owned()),
        Field::Null => Err(invalid(format!("column {name} is null"))),
        other => Err(invalid(format!(
            "column {name} has non-string field {other:?}"
        ))),
    }
}

fn raw_u8(row: &Row, name: &str) -> AppResult<u8> {
    let value = i64_value(row, name)?;
    u8::try_from(value).map_err(|_| invalid(format!("column {name} value {value} does not fit u8")))
}

fn numeric_symbol_code(symbol: &str) -> AppResult<i32> {
    let symbol = symbol.trim_matches('\0').trim();
    let digits = symbol
        .chars()
        .filter(|ch| ch.is_ascii_digit())
        .take(6)
        .collect::<String>();
    if digits.len() != 6 {
        return Err(invalid(format!(
            "symbol {symbol:?} does not contain a six-digit code"
        )));
    }
    let base = digits
        .parse::<i32>()
        .map_err(|source| invalid(format!("symbol {symbol:?} numeric parse failed: {source}")))?;
    let venue_prefix = if symbol.ends_with(".SH") || symbol.ends_with(".XSHG") {
        1
    } else if symbol.ends_with(".SZ") || symbol.ends_with(".XSHE") {
        2
    } else {
        9
    };
    Ok(venue_prefix * 1_000_000 + base)
}

fn day_key_from_raw_time(raw_time: i64) -> i32 {
    let mut value = raw_time.unsigned_abs();
    while value >= 100_000_000 {
        value /= 10;
    }
    i32::try_from(value).unwrap_or(0)
}

fn scaled_price_cents(price: f64) -> i64 {
    if price.is_finite() && price > 0.0 {
        (price * 100.0).round() as i64
    } else {
        0
    }
}

fn scaled_volume(volume: f64) -> i64 {
    if volume.is_finite() && volume > 0.0 {
        volume.round() as i64
    } else {
        0
    }
}

fn venue_code(exchange_id: i64) -> i32 {
    match exchange_id {
        EXCHANGE_XSHG => 1,
        EXCHANGE_XSHE => 2,
        _ => 0,
    }
}

fn order_event_kind(raw: u8) -> i32 {
    match raw {
        ORDER_TYPE_DELETE => 2,
        ORDER_TYPE_STATE => 4,
        ORDER_TYPE_ANY_PRICE | ORDER_TYPE_LIMIT_PRICE | ORDER_TYPE_BEST_PRICE | ORDER_TYPE_ADD => 1,
        _ => 0,
    }
}

fn trade_event_kind(raw: u8) -> i32 {
    match raw {
        TRADE_TYPE_CANCEL => 2,
        TRADE_TYPE_TRADE => 3,
        _ => 0,
    }
}

fn order_side_code(raw: u8) -> i32 {
    match raw {
        ORDER_SIDE_BUY => 1,
        ORDER_SIDE_SELL => 2,
        _ => 0,
    }
}

fn trade_side_code(raw: u8) -> i32 {
    match raw {
        TRADE_SIDE_UNKNOWN => 0,
        TRADE_SIDE_BUY => 1,
        TRADE_SIDE_SELL => 2,
        _ => 0,
    }
}

fn parse_value<T>(args: &mut impl Iterator<Item = String>, flag: &str) -> AppResult<T>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
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
        "usage: l2_parquet_to_ocb --day-dir YYYYMMDD_DIR [--output-dir DIR] [--row-limit N|--all-rows] [--overwrite]\n\
         \n\
         optional explicit paths:\n\
           --orders L2ORDER.journal --trades L2TRADE.journal --market-data L2MD.journal\n\
           --order-trade-output out-order-trade.ocb --market-data-output out-market-data.ocb\n\
         \n\
         --only both|order-trade|market-data defaults to both. The default row limit is {DEFAULT_ROW_LIMIT};\n\
         pass --all-rows only for small days or after budgeting memory for this in-memory example."
    );
}

fn path_io(action: &str, path: &Path, source: io::Error) -> Box<dyn Error> {
    invalid(format!("{action} {}: {source}", path.display()))
}

fn invalid(message: impl Into<String>) -> Box<dyn Error> {
    Box::new(io::Error::new(ErrorKind::InvalidInput, message.into()))
}
