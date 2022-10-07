use poise::serenity_prelude as serenity;

use rusqlite;

type Error = Box<dyn std::error::Error + Send + Sync>;
type Context<'a> = poise::Context<'a, Data, Error>;
// User data, which is stored and accessible in all command invocations
struct Data {}

thread_local!(static CONN: rusqlite::Connection = rusqlite::Connection::open("DB.db").unwrap());

#[poise::command(prefix_command)]
async fn register(ctx: Context<'_>) -> Result<(), Error> {
    poise::builtins::register_application_commands_buttons(ctx).await?;
    Ok(())
}

async fn eph_reply<'a>(
    ctx: Context<'a>,
    msg: impl ToString,
) -> Result<poise::ReplyHandle<'a>, serenity::Error> {
    ctx.send(|r| r.content(msg.to_string()).ephemeral(true))
        .await
}

#[poise::command(slash_command, prefix_command)]
async fn adduser(
    ctx: Context<'_>,
    #[description = "Email"] email: String,
    #[description = "Password"] password: String,
) -> Result<(), Error> {
    let r = CONN.with(|c| {
        c.execute(
            "INSERT INTO users VALUES (?, ?, ?)",
            (&email, &password, &ctx.author().id.to_string()),
        )
    });
    if let Err(rusqlite::Error::SqliteFailure(
        rusqlite::ffi::Error {
            code: rusqlite::ErrorCode::ConstraintViolation,
            extended_code: _,
        },
        m,
    )) = r
    {
        eph_reply(ctx, "oopy daisy").await?;
        return Ok(());
    }
    if let Err(e) = r {
        eprintln!("DB Error: {}", e);
        eph_reply(ctx, format!("Database error occured: {}", e)).await?;
        return Ok(());
    }
    ctx.say(format!("HI. {}, {}", email, password)).await?;
    Ok(())
}

#[tokio::main]
async fn main() {
    let framework = poise::Framework::builder()
        .options(poise::FrameworkOptions {
            prefix_options: poise::PrefixFrameworkOptions {
                prefix: Some(String::from("~")),
                ..Default::default()
            },
            commands: vec![register(), adduser()],
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
