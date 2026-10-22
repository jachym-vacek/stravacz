//! A fake Strava.cz for developing frontends without a real account.
//!
//! ```sh
//! cargo run -p stravacz-api --example mock_server
//! STRAVA_BASE_URL=http://127.0.0.1:8555 cargo run -p stravacz-tui
//! ```
//!
//! Log in with any credentials. Orders are kept in memory until it exits.

use std::sync::Mutex;

use chrono::{Datelike, Local, TimeDelta, Weekday};
use serde_json::{Value, json};
use wiremock::matchers::method;
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

const MAINS: &[&str] = &[
    "Kuřecí řízek, bramborová kaše, okurkový salát",
    "Špagety s boloňskou omáčkou, sýr",
    "Zeleninové rizoto s parmazánem",
    "Vepřová pečeně, knedlík, zelí",
    "Smažený sýr, vařené brambory, tatarská omáčka",
    "Hovězí guláš, houskový knedlík",
    "Rybí filé na másle, bramborová kaše",
];
const SOUPS: &[&str] = &[
    "Hovězí vývar s kapáním",
    "Česnečka s krutony",
    "Kulajda",
    "Bramboračka",
    "Zeleninová polévka s kroupami",
];

#[tokio::main]
async fn main() {
    let listener = std::net::TcpListener::bind("127.0.0.1:8555").expect("port 8555 is free");
    let server = MockServer::builder().listener(listener).start().await;

    Mock::given(method("POST"))
        .respond_with(Canteen::default())
        .mount(&server)
        .await;

    println!("Mock Strava.cz listening on {}", server.uri());
    tokio::signal::ctrl_c().await.expect("ctrl-c handler");
}

fn login() -> Value {
    json!({
        "sid": "MOCKSESSION",
        "s5url": "https://example.invalid/WSStravne5.svc",
        "uzivatel": {
            "konto": "512.50",
            "jmeno": "Jan Novák",
            "mena": "Kč",
            "nazevJidelny": "Školní jídelna U Lípy"
        }
    })
}

fn menu() -> Value {
    let today = Local::now().date_naive();
    let mut tables = serde_json::Map::new();
    let mut id = 100;

    for offset in -2..19 {
        let date = today + TimeDelta::days(offset);
        if matches!(date.weekday(), Weekday::Sat | Weekday::Sun) {
            continue;
        }
        let datum = date.format("%d.%m.%Y").to_string();
        let index = usize::try_from(date.ordinal()).unwrap();
        let closed = if offset < 1 { "C" } else { "" };
        let holiday = date.weekday() == Weekday::Wed && offset > 7 && offset < 14;
        let day_code = if holiday {
            "VP"
        } else if offset > 14 {
            "T"
        } else {
            ""
        };

        let mut row = |druh: &str,
                       label: &str,
                       name: &str,
                       price: &str,
                       ordered: bool,
                       obj: &str| {
            id += 1;
            json!({
                "datum": datum, "druh": druh, "druh_popis": label, "nazev": name,
                "cena": price, "pocet": u8::from(ordered), "veta": id.to_string(),
                "alergeny": [["01", "Obiloviny obsahující lepek"], ["07", "Mléko"], ["09", "Celer"]],
                "omezeniObj": { "den": day_code, "obj": obj, "zm": closed },
                "casKonec": (date - TimeDelta::days(1)).format("%Y-%m-%dT14:00:00").to_string(),
            })
        };

        let rows = if holiday {
            vec![row("O1", "Oběd 1", "Státní svátek", "0", false, "V")]
        } else {
            let lunch = index % 3 == 0 && offset < 6;
            vec![
                row("PO", "Polévka", SOUPS[index % SOUPS.len()], "0", lunch, "I"),
                row(
                    "O1",
                    "Oběd 1",
                    MAINS[index % MAINS.len()],
                    "52.00",
                    lunch,
                    closed,
                ),
                row(
                    "O2",
                    "Oběd 2",
                    MAINS[(index + 3) % MAINS.len()],
                    "52.00",
                    false,
                    closed,
                ),
                row(
                    "O3",
                    "Oběd 3",
                    MAINS[(index + 5) % MAINS.len()],
                    "58.50",
                    false,
                    closed,
                ),
            ]
        };
        tables.insert(format!("table{}", tables.len()), Value::Array(rows));
    }
    Value::Object(tables)
}

/// Remembers orders like the real canteen: staged by `pridejJidloS5`,
/// committed by `saveOrders`, dropped by `nactiVlastnostiPA`.
struct Canteen {
    state: Mutex<State>,
}

struct State {
    menu: Value,
    staged: Vec<(String, u8)>,
}

impl Default for Canteen {
    fn default() -> Self {
        Self {
            state: Mutex::new(State {
                menu: menu(),
                staged: Vec::new(),
            }),
        }
    }
}

impl Respond for Canteen {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        let body: Value = serde_json::from_slice(&request.body).unwrap_or_default();
        let mut state = self.state.lock().unwrap();

        let response = match request.url.path() {
            "/api/login" => login(),
            "/api/objednavky" => state.menu.clone(),
            "/api/pridejJidloS5" => {
                let veta = body["veta"].as_str().unwrap_or_default().to_owned();
                state.staged.push((veta, u8::from(body["pocet"] == "1")));
                json!({})
            }
            "/api/saveOrders" => {
                let staged = std::mem::take(&mut state.staged);
                let rows = state.menu.as_object_mut().unwrap().values_mut();
                for row in rows.filter_map(Value::as_array_mut).flatten() {
                    if let Some((_, pocet)) = staged.iter().find(|(veta, _)| row["veta"] == *veta) {
                        row["pocet"] = json!(pocet);
                    }
                }
                json!({})
            }
            "/api/nactiVlastnostiPA" => {
                state.staged.clear();
                json!({})
            }
            "/api/logOut" => json!(true),
            _ => return ResponseTemplate::new(404),
        };
        ResponseTemplate::new(200).set_body_json(response)
    }
}
