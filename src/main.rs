use log::info;
use serde_json::json;
use teloxide::{prelude::*, utils::command::BotCommands};

#[tokio::main]
async fn main() {
    dotenv::dotenv().ok();
    std::env::set_var("RUST_LOG", "info");
    pretty_env_logger::init();

    log::info!("Starting command bot...");

    let bot = Bot::from_env();

    Command::repl(bot, answer).await;
}

#[derive(BotCommands, Clone)]
#[command(
    rename_rule = "lowercase",
    description = "These commands are supported:"
)]
enum Command {
    #[command(description = "polonista command")]
    P,
}

async fn answer(bot: Bot, msg: Message, cmd: Command) -> ResponseResult<()> {
    match cmd {
        Command::P => {
            // Check if the message is from user ID 5337682436 or group id -4132877256
            if msg.from().unwrap().id != teloxide::prelude::UserId(5337682436) && msg.chat.id != teloxide::prelude::ChatId(-4132877256) {
                info!("Ignoring message from user: {:?}", msg.from().unwrap().id);
                return Ok(());
            }
            // Remove the first word (bot prefix)
            let text = msg.text().unwrap();
            let text = text.split_whitespace().skip(1).collect::<Vec<&str>>().join(" ");
            info!("Received a message: {:?}", text);
            // Create a thread
            let client = reqwest::Client::new();
            let response = client
                .post("https://api.openai.com/v1/threads")
                .bearer_auth(std::env::var("OPENAI_API_TOKEN").unwrap())
                .header("OpenAI-Beta", "assistants=v2")
                .json(&json!({
                    "messages": [
                        {"role": "user", "content": text}
                    ]
                }))
                .send()
                .await
                .unwrap();

            let thread_id = response.json::<serde_json::Value>().await.unwrap();
            let thread_id = thread_id["id"].as_str().expect("Failed to get thread id");

            // Run a run
            let response = client
                .post(format!(
                    "https://api.openai.com/v1/threads/{}/runs",
                    thread_id
                ))
                .bearer_auth(std::env::var("OPENAI_API_TOKEN").unwrap())
                .header("OpenAI-Beta", "assistants=v2")
                .json(&json!({
                    "assistant_id": "asst_CUwDLhqfxGWY3JXMMwGMzjG5",
                    "temperature": 0.6, // lower temperature -> less hallucinations
                }))
                .send()
                .await
                .unwrap();

            let response = response.json::<serde_json::Value>().await.unwrap();

            // Check if the status is queued or in_progress
            let bot_msg = if response["status"] == "queued" {
                bot.send_message(msg.chat.id, "I'm thinking...")
                .reply_to_message_id(msg.id)
                .await?
            } else {
                bot
                    .send_message(msg.chat.id, "Something went wrong. Please try again later. @DuckyBlender pls fix")
                    .reply_to_message_id(msg.id)
                    .await?;
                return Ok(());
            };

            let run_id = response["id"].as_str().unwrap();

            // Query the status every 0.5 second
            loop {
                info!("Checking status");
                let response = client
                    .get(format!(
                        // https://api.openai.com/v1/threads/{thread_id}/runs/{run_id}
                        "https://api.openai.com/v1/threads/{}/runs/{}",
                        thread_id,
                        run_id
                    ))
                    .bearer_auth(std::env::var("OPENAI_API_TOKEN").unwrap())
                    .header("OpenAI-Beta", "assistants=v2")
                    .send()
                    .await
                    .unwrap();

                let response = response.json::<serde_json::Value>().await.unwrap();
                let status = response["status"].as_str().unwrap();

                if status == "completed" {
                    info!("Breaking out of loop");
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            }

            let response = client
                .get(format!(
                    "https://api.openai.com/v1/threads/{thread_id}/messages",
                ))
                .header("OpenAI-Beta", "assistants=v2")
                .bearer_auth(std::env::var("OPENAI_API_TOKEN").unwrap())
                .send()
                .await
                .unwrap();
                  
            let response = response.json::<serde_json::Value>().await.unwrap();
            let content = response["data"][0]["content"][0]["text"]["value"].as_str().unwrap();
            // Remove text between 【】including the characters
            let re = regex::Regex::new(r"【.*?】").unwrap();
            let content = re.replace_all(content, "");
            bot.edit_message_text(msg.chat.id, bot_msg.id, content)
                .await?;
        }
    };

    Ok(())
}
