use poise::serenity_prelude as serenity;
use std::collections::HashMap;
use thiserror::Error;

use reqwest;
use rusqlite;

const BASE: &str = "https://app.enrichingstudents.com/";
const BASE2: &str = "https://student.enrichingstudents.com/";
const UA: &str =
    "EnrichingStudentsBot/v0.1 (https://github.com/reallygoodaccount1/enriching-students-bot)";

type Error = Box<dyn std::error::Error + Send + Sync>;
type Context<'a> = poise::Context<'a, Data, Error>;
// User data, which is stored and accessible in all command invocations
struct Data {}

#[derive(serde::Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ViewModel {
    token1: String,
    token2: String,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "PascalCase")]
struct LoginResponse {
    error_messages: Vec<String>,
    is_authorized: bool,
    view_model: ViewModel,
}

#[derive(serde::Deserialize)]
struct LoginResponse2 {
    #[serde(rename = "authToken")]
    auth_token: String,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct ScheduleInfo {
    period_description: String,
    course_name: String,
}

#[derive(serde::Deserialize)]
struct CourseInfoResponse {
    courses: Vec<CourseInfo>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct CourseInfo {
    blocked_reason: Option<String>,
    prevent_student_self_scheduling: bool,
    is_open_for_scheduling: bool,
    course_id: u64,
    course_name: String,
    staff_last_name: String,
    staff_first_name: String,
    course_room: String,
    max_number_students: u64,
    number_of_appointments: u64,
    appointment_request_course_id: u64,
}

#[derive(Error, Debug)]
enum APIError {
    #[error("reqwest error")]
    ReqwestError(#[from] reqwest::Error),
    #[error("api error")]
    APIError(String),
}

struct API {
    s: reqwest::Client,
    email: String,
    password: String,
    token: Option<String>,
}

impl API {
    fn new(email: String, password: String) -> Self {
        Self {
            s: reqwest::ClientBuilder::new()
                .user_agent(UA)
                .build()
                .unwrap(),
            email,
            password,
            token: Option::None,
        }
    }

    async fn login(&mut self) -> Result<(), reqwest::Error> {
        let r1 = self
            .s
            .post(BASE.to_owned() + "/LoginApi/Validate")
            .json(&HashMap::from([(
                "parameters",
                HashMap::from([("EmailAddress", &self.email), ("Password", &self.password)]),
            )]))
            .send()
            .await?
            .json::<LoginResponse>()
            .await?;

        let r2 = self
            .s
            .post(BASE2.to_owned() + "/v1.0/login/viatokens")
            .json(&HashMap::from([
                ("token1", r1.view_model.token1),
                ("token2", r1.view_model.token2),
            ]))
            .send()
            .await?
            .json::<LoginResponse2>()
            .await?;

        self.token = Some(r2.auth_token);

        //println!("{:?}", r2.json::<serde_json::Value>().await);
        Ok(())
    }

    async fn get_course(self, date: String) -> Result<String, reqwest::Error> {
        let r = self
            .s
            .post(BASE2.to_owned() + "/v1.0/appointment/viewschedule")
            .json(&HashMap::from([("startDate", date)]))
            .headers(self.headers())
            .send()
            .await?
            .json::<Vec<ScheduleInfo>>()
            .await?;

        let rf = r
            .into_iter()
            .filter(|x| x.period_description == "Centaur Plus Enrichment")
            .collect::<Vec<ScheduleInfo>>();
        if rf.len() > 1 {
            panic!("major issue!!!")
        }

        Ok(rf.into_iter().nth(0).unwrap().course_name)
    }

    async fn get_course_id(&self, date: String, course_name: String) -> Result<u64, APIError> {
        let r = self
            .s
            .post(BASE2.to_owned() + "/v1.0/course/forstudentscheduling")
            .json(&HashMap::from([
                ("periodId", serde_json::Value::Number(1.into())),
                ("startDate", serde_json::Value::String(date)),
            ]))
            .headers(self.headers())
            .send()
            .await?
            .json::<CourseInfoResponse>()
            .await?;

        for x in r.courses {
            if x.course_name == course_name {
                return Ok(x.course_id);
            }
        }
        Err(APIError::APIError("course not found".to_string()))
    }

    async fn schedule(self, date: String, cid: u64, comment: String) -> Result<(), reqwest::Error> {
        let r = self
            .s
            .post(BASE2.to_string() + "/v1.0/appointment/save")
            .json(&HashMap::from([
                ("courseId", serde_json::Value::Number(cid.into())),
                ("periodId", serde_json::Value::Number(1.into())),
                ("scheduleDate", serde_json::Value::String(date)),
                ("schedulerComment", serde_json::Value::String(comment)),
            ]))
            .headers(self.headers())
            .send()
            .await?;
        Ok(())
    }

    fn headers(&self) -> reqwest::header::HeaderMap {
        let mut map = reqwest::header::HeaderMap::new();

        map.insert("User-Agent", UA.parse().unwrap());
        map.insert(
            "ESAuthToken",
            self.token.clone().expect("missing token")[..]
                .parse()
                .unwrap(),
        );
        map.insert(
            "Referer",
            (BASE2.to_owned() + "/dashboard")[..].parse().unwrap(),
        );

        map
    }
}

async fn eph_reply<'a>(
    ctx: Context<'a>,
    msg: impl ToString,
) -> Result<poise::ReplyHandle<'a>, serenity::Error> {
    ctx.send(|r| r.content(msg.to_string()).ephemeral(true))
        .await
}

thread_local!(static CONN: rusqlite::Connection = rusqlite::Connection::open("DB.db").unwrap());

#[poise::command(prefix_command)]
async fn registercmds(ctx: Context<'_>) -> Result<(), Error> {
    poise::builtins::register_application_commands_buttons(ctx).await?;
    Ok(())
}

#[poise::command(slash_command, prefix_command)]
async fn register(
    ctx: Context<'_>,
    #[description = "Email"] email: String,
    #[description = "Password"] password: String,
) -> Result<(), Error> {
    let r = CONN.with(|c| {
        c.execute(
            "INSERT INTO users VALUES (?, ?, ?) ON CONFLICT(id) DO UPDATE SET email=excluded.email, pass=excluded.pass",
            (&email, &password, &ctx.author().id.to_string()),
        )
    });
    if let Err(e) = r {
        eprintln!("DB Error: {}", e);
        eph_reply(ctx, format!("Database error occured: {}", e)).await?;
        return Ok(());
    }
    eph_reply(ctx, format!("HI. {}, {}", email, password)).await?;
    Ok(())
}

#[poise::command(slash_command, prefix_command)]
async fn schedule(
    ctx: Context<'_>,
    #[description = "Course Name"] course_name: String,
    #[description = "Date"] date: String,
    #[description = "Comment"] comment: String,
) -> Result<(), Error> {
    let (email, password) = getuser(ctx.author().id.to_string());
    let mut api = API::new(email, password);
    api.login().await.expect("error");
    let cid = api.get_course_id(date.clone(), course_name.clone()).await?;
    api.schedule(date.clone(), cid, comment).await?;
    eph_reply(
        ctx,
        format!("You are scheduled for {} on {}.", course_name, date),
    )
    .await?;
    Ok(())
}

#[poise::command(slash_command, prefix_command)]
async fn getcourse(ctx: Context<'_>, #[description = "Date"] date: String) -> Result<(), Error> {
    let (email, password) = getuser(ctx.author().id.to_string());
    let mut api = API::new(email, password);
    api.login().await.expect("error");
    eph_reply(
        ctx,
        format!(
            "You are scheduled for {} on {}.",
            api.get_course(date.clone()).await.unwrap(),
            &date
        ),
    )
    .await?;
    Ok(())
}

fn getuser(id: String) -> (String, String) {
    CONN.with(|c| {
        c.query_row("SELECT email, pass FROM users WHERE id=?", [id], |row| {
            Ok((row.get_unwrap(0), row.get_unwrap(1)))
        })
    })
    .unwrap()
}

#[tokio::main]
async fn main() {
    let framework = poise::Framework::builder()
        .options(poise::FrameworkOptions {
            prefix_options: poise::PrefixFrameworkOptions {
                prefix: Some(String::from("~")),
                ..Default::default()
            },
            commands: vec![registercmds(), register(), getcourse(), schedule()],
            ..Default::default()
        })
        .token(String::from(include_str!("../token.txt")))
        .intents(
            serenity::GatewayIntents::non_privileged()
                | serenity::GatewayIntents::GUILD_MESSAGES
                | serenity::GatewayIntents::DIRECT_MESSAGES
                | serenity::GatewayIntents::MESSAGE_CONTENT,
        )
        .user_data_setup(move |_ctx, _ready, _framework| Box::pin(async move { Ok(Data {}) }));

    framework.run().await.unwrap();
}
