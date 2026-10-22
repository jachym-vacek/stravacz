use chrono::NaiveDate;
use serde_json::{Value, json};
use stravacz_api::{
    Change, Client, Credentials, DayStatus, Error, MealId, MealKind, Money, Restriction, Session,
};
use wiremock::matchers::{body_partial_json, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn fixture(name: &str) -> Value {
    let text = match name {
        "login" => include_str!("fixtures/login.json"),
        "menu" => include_str!("fixtures/menu.json"),
        _ => unreachable!(),
    };
    serde_json::from_str(text).unwrap()
}

fn ok(body: Value) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(body)
}

async fn logged_in() -> (MockServer, Session) {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/login"))
        .and(body_partial_json(
            json!({ "cislo": "1234", "jmeno": "jan.novak" }),
        ))
        .respond_with(ok(fixture("login")))
        .mount(&server)
        .await;

    let session = Client::with_base_url(&server.uri())
        .unwrap()
        .login(&Credentials::new("1234", "jan.novak", "secret"))
        .await
        .unwrap();
    (server, session)
}

#[tokio::test]
async fn login_reads_the_account() {
    let (_server, session) = logged_in().await;
    let account = session.account();

    assert_eq!(account.full_name, "Jan Novák");
    assert_eq!(account.canteen_name.as_deref(), Some("Školní jídelna Test"));
    assert_eq!(account.balance, Money::from_hundredths(51_250));
    assert_eq!(account.currency, "Kč");
}

#[tokio::test]
async fn login_maps_the_error_envelope() {
    let server = MockServer::start().await;
    Mock::given(path("/api/login"))
        .respond_with(ResponseTemplate::new(555).set_body_json(json!({
            "state": "error", "number": 20, "message": "Wrong nickname or password"
        })))
        .mount(&server)
        .await;

    let err = Client::with_base_url(&server.uri())
        .unwrap()
        .login(&Credentials::new("1234", "jan.novak", "wrong"))
        .await
        .unwrap_err();
    assert!(err.is_authentication(), "{err:?}");
    assert_eq!(err.to_string(), "Wrong nickname or password");
}

#[tokio::test]
async fn menu_is_parsed_leniently() {
    let (server, session) = logged_in().await;
    Mock::given(path("/api/objednavky"))
        .and(body_partial_json(json!({ "sid": "0123456789ABCDEF" })))
        .respond_with(ok(fixture("menu")))
        .mount(&server)
        .await;

    let menu = session.menu().await.unwrap();
    let date = |day| NaiveDate::from_ymd_opt(2026, 9, day).unwrap();

    let days = menu.days();
    assert_eq!(days.len(), 3);
    assert_eq!(days[0].status, DayStatus::NoService);
    assert!(days[0].meals.is_empty() && !days[0].is_service_day());

    // The unpublished "Oběd 2" slot is dropped.
    let wednesday = menu.day(date(2)).unwrap();
    assert_eq!(wednesday.meals.len(), 2);
    let soup = &wednesday.meals[0];
    assert_eq!(soup.kind, MealKind::Soup);
    assert_eq!(soup.order_restriction, Restriction::NotSeparately);
    assert_eq!(soup.allergens.len(), 2);
    let lunch = menu.meal(MealId(86)).unwrap();
    assert!(lunch.ordered && lunch.can_cancel());
    assert_eq!(lunch.price, Money::from_hundredths(5000));
    assert!(lunch.deadline.is_some());

    assert_eq!(menu.day(date(3)).unwrap().status, DayStatus::NotAutoOrdered);
    assert!(!menu.meal(MealId(89)).unwrap().can_order());
}

#[tokio::test]
async fn submit_stages_saves_and_verifies() {
    let (server, session) = logged_in().await;
    Mock::given(path("/api/pridejJidloS5"))
        .and(body_partial_json(json!({ "veta": "88", "pocet": "1", "url": "https://wss5.strava.cz/WSStravne5_15/WSStravne5.svc" })))
        .respond_with(ok(json!({ "konto": "460.00" })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(path("/api/saveOrders"))
        .respond_with(ok(json!({})))
        .expect(1)
        .mount(&server)
        .await;
    // The canteen still reports meal 88 as not ordered.
    Mock::given(path("/api/objednavky"))
        .respond_with(ok(fixture("menu")))
        .mount(&server)
        .await;

    let submission = session.submit(&[Change::order(MealId(88))]).await.unwrap();
    assert_eq!(submission.balance, Some(Money::from_hundredths(46_000)));
    assert_eq!(submission.rejected, [MealId(88)]);
}

#[tokio::test]
async fn failed_submit_discards_staged_changes() {
    let (server, session) = logged_in().await;
    Mock::given(path("/api/pridejJidloS5"))
        .respond_with(ResponseTemplate::new(555).set_body_json(json!({
            "state": "error", "number": 35, "message": "Nedostatečná výše konta"
        })))
        .mount(&server)
        .await;
    Mock::given(path("/api/nactiVlastnostiPA"))
        .respond_with(ok(json!({})))
        .expect(1)
        .mount(&server)
        .await;

    let err = session
        .submit(&[Change::order(MealId(88))])
        .await
        .unwrap_err();
    assert!(matches!(err, Error::InsufficientBalance(_)), "{err:?}");
}
