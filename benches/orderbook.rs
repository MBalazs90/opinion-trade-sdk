use criterion::{Criterion, black_box, criterion_group, criterion_main};
use opinion_trade_sdk::fixed_book::{FixedOrderBook, parse_price_fixed, parse_size_fixed};
use opinion_trade_sdk::models::{OrderBook, OrderBookLevel};
use opinion_trade_sdk::orderbook::LocalOrderBook;
use opinion_trade_sdk::types::Side;
use serde_json::json;

fn make_level(price: &str, size: &str) -> OrderBookLevel {
    OrderBookLevel {
        price: price.into(),
        size: size.into(),
    }
}

fn make_orderbook(levels: usize) -> OrderBook {
    let mut bids = Vec::with_capacity(levels);
    let mut asks = Vec::with_capacity(levels);
    for i in 0..levels {
        let bid_price = format!("{:.2}", 0.50 - i as f64 * 0.01);
        let ask_price = format!("{:.2}", 0.55 + i as f64 * 0.01);
        let size = format!("{}", 100 + i * 10);
        bids.push(make_level(&bid_price, &size));
        asks.push(make_level(&ask_price, &size));
    }
    OrderBook {
        market: Some("bench".into()),
        token_id: Some("tok_bench".into()),
        bids,
        asks,
        timestamp: Some(1700000000),
        extra: json!({}),
    }
}

fn bench_from_rest(c: &mut Criterion) {
    let ob = make_orderbook(50);

    let mut group = c.benchmark_group("from_rest");
    group.bench_function("LocalOrderBook (f64)", |b| {
        b.iter(|| LocalOrderBook::from_rest(black_box(&ob)))
    });
    group.bench_function("FixedOrderBook (u32/i64)", |b| {
        b.iter(|| FixedOrderBook::from_rest(black_box(&ob)))
    });
    group.finish();
}

fn bench_apply_delta(c: &mut Criterion) {
    let ob = make_orderbook(50);

    let mut group = c.benchmark_group("apply_delta");
    group.bench_function("LocalOrderBook (string side + f64)", |b| {
        let mut book = LocalOrderBook::from_rest(&ob);
        b.iter(|| {
            book.apply_delta(black_box("buy"), black_box(0.42), black_box(500.0));
        })
    });
    group.bench_function("FixedOrderBook (string side + f64)", |b| {
        let mut book = FixedOrderBook::from_rest(&ob);
        b.iter(|| {
            book.apply_delta(black_box("buy"), black_box(0.42), black_box(500.0));
        })
    });
    group.bench_function("FixedOrderBook (u8 + u32/i64 hot path)", |b| {
        let mut book = FixedOrderBook::from_rest(&ob);
        b.iter(|| {
            book.apply_delta_fixed(black_box(0), black_box(4200), black_box(500_000_000));
        })
    });
    group.finish();
}

fn bench_best_bid_ask(c: &mut Criterion) {
    let ob = make_orderbook(50);

    let mut group = c.benchmark_group("best_bid_ask");
    group.bench_function("LocalOrderBook", |b| {
        let book = LocalOrderBook::from_rest(&ob);
        b.iter(|| {
            black_box(book.best_bid());
            black_box(book.best_ask());
        })
    });
    group.bench_function("FixedOrderBook (f64 conversion)", |b| {
        let book = FixedOrderBook::from_rest(&ob);
        b.iter(|| {
            black_box(book.best_bid());
            black_box(book.best_ask());
        })
    });
    group.bench_function("FixedOrderBook (raw u32)", |b| {
        let book = FixedOrderBook::from_rest(&ob);
        b.iter(|| {
            black_box(book.best_bid_fixed());
            black_box(book.best_ask_fixed());
        })
    });
    group.finish();
}

fn bench_calculate_market_price(c: &mut Criterion) {
    let ob = make_orderbook(50);

    let mut group = c.benchmark_group("calculate_market_price");
    group.bench_function("LocalOrderBook", |b| {
        let book = LocalOrderBook::from_rest(&ob);
        b.iter(|| book.calculate_market_price(black_box(Side::Buy), black_box(500.0)))
    });
    group.bench_function("FixedOrderBook", |b| {
        let book = FixedOrderBook::from_rest(&ob);
        b.iter(|| book.calculate_market_price(black_box(Side::Buy), black_box(500.0)))
    });
    group.finish();
}

fn bench_simulate_fill(c: &mut Criterion) {
    let ob = make_orderbook(50);

    let mut group = c.benchmark_group("simulate_fill");
    group.bench_function("LocalOrderBook", |b| {
        let book = LocalOrderBook::from_rest(&ob);
        b.iter(|| {
            book.simulate_fill(
                black_box(Side::Buy),
                black_box(500.0),
                black_box(Some(0.05)),
            )
        })
    });
    group.bench_function("FixedOrderBook", |b| {
        let book = FixedOrderBook::from_rest(&ob);
        b.iter(|| {
            book.simulate_fill(
                black_box(Side::Buy),
                black_box(500.0),
                black_box(Some(0.05)),
            )
        })
    });
    group.finish();
}

fn bench_spread(c: &mut Criterion) {
    let ob = make_orderbook(50);

    let mut group = c.benchmark_group("spread");
    group.bench_function("LocalOrderBook (f64)", |b| {
        let book = LocalOrderBook::from_rest(&ob);
        b.iter(|| black_box(book.spread()))
    });
    group.bench_function("FixedOrderBook (f64)", |b| {
        let book = FixedOrderBook::from_rest(&ob);
        b.iter(|| black_box(book.spread()))
    });
    group.bench_function("FixedOrderBook (u32 raw)", |b| {
        let book = FixedOrderBook::from_rest(&ob);
        b.iter(|| black_box(book.spread_fixed()))
    });
    group.finish();
}

fn bench_parse_price(c: &mut Criterion) {
    let mut group = c.benchmark_group("parse_price");
    group.bench_function("str::parse::<f64>", |b| {
        b.iter(|| black_box("0.5500").parse::<f64>())
    });
    group.bench_function("parse_price_fixed (custom)", |b| {
        b.iter(|| parse_price_fixed(black_box("0.5500")))
    });
    group.finish();
}

fn bench_parse_size(c: &mut Criterion) {
    let mut group = c.benchmark_group("parse_size");
    group.bench_function("str::parse::<f64>", |b| {
        b.iter(|| black_box("1234.567890").parse::<f64>())
    });
    group.bench_function("parse_size_fixed (custom)", |b| {
        b.iter(|| parse_size_fixed(black_box("1234.567890")))
    });
    group.finish();
}

fn bench_ws_event_parsing(c: &mut Criterion) {
    let ob_event = json!({
        "channel": "orderbook",
        "marketId": 42,
        "bids": [
            {"price": "0.50", "size": "100"},
            {"price": "0.49", "size": "200"},
            {"price": "0.48", "size": "150"},
        ],
        "asks": [
            {"price": "0.55", "size": "100"},
            {"price": "0.56", "size": "200"},
            {"price": "0.57", "size": "150"},
        ]
    });

    let event = opinion_trade_sdk::WsEvent::OrderBook {
        market_id: Some(42),
        data: ob_event,
    };

    let mut group = c.benchmark_group("ws_book_apply");
    group.bench_function("BookApplier (f64)", |b| {
        let book = LocalOrderBook::new("tok_1");
        let mut applier = opinion_trade_sdk::BookApplier::new(book);
        b.iter(|| applier.apply_event(black_box(&event)))
    });
    group.bench_function("FastBookApplier (fixed-point)", |b| {
        let book = FixedOrderBook::new("tok_1");
        let mut applier = opinion_trade_sdk::FastBookApplier::new(book);
        b.iter(|| applier.apply_event(black_box(&event)))
    });
    group.finish();
}

criterion_group!(
    benches,
    bench_parse_price,
    bench_parse_size,
    bench_from_rest,
    bench_apply_delta,
    bench_best_bid_ask,
    bench_spread,
    bench_calculate_market_price,
    bench_simulate_fill,
    bench_ws_event_parsing,
);
criterion_main!(benches);
