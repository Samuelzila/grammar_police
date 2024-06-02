use serde_json as json;
use serenity::all::{
    CreateCommand, CreateCommandOption, CreateInteractionResponse,
    CreateInteractionResponseMessage, Interaction, Ready, User,
};
use serenity::model::channel::Message;
use serenity::prelude::*;
use serenity::{async_trait, model};
use tokio::fs;

struct Handler;
#[async_trait]
impl EventHandler for Handler {
    async fn ready(&self, ctx: Context, _ready: Ready) {
        //Register commands
        let builder = CreateCommand::new("grammar_enable")
            .description("Enable grammar policing for yourself and only yourself.");
        model::application::Command::create_global_command(&ctx, builder)
            .await
            .unwrap();
        let builder = CreateCommand::new("grammar_add_word")
            .description("Add word to dictionary.")
            .add_option(CreateCommandOption::new(
                serenity::all::CommandOptionType::String,
                "word",
                "Word to add to dictionary.",
            ));
        model::application::Command::create_global_command(&ctx, builder)
            .await
            .unwrap();
    }

    async fn message(&self, ctx: Context, msg: Message) {
        //Only correct authorised users.
        if is_willing_user(&msg.author).await {
            //Message to be sent to the user
            let mut response_message = String::new();

            //Make http request to local LanguageTools server and parse it to json.
            let response: json::Value = json::from_str(
                &reqwest::get(format!(
                    "http://localhost:8081/v2/check?language={}&text={}",
                    "fr-CA", msg.content
                ))
                .await
                .expect("LanguageTool request failed")
                .text()
                .await
                .unwrap(),
            )
            .expect("Could not parse LanguageTool response as json.");

            let matches = response["matches"].as_array().unwrap();

            //Only do stuff if mistakes were found.
            if matches.len() != 0 {
                //Manage matches and generate response text.
                response_message.push_str("Halte-là !\n\n");

                for mistake in matches {
                    response_message.push_str(&format!(
                        "« {} ». {}\n",
                        mistake["context"]["text"].as_str().unwrap(),
                        mistake["message"].as_str().unwrap()
                    ));

                    let replacements = mistake["replacements"].as_array().unwrap();
                    if replacements.len() != 0 {
                        response_message.push_str("Voici des corrections possibles:\n");
                        for i in 0..3.min(replacements.len()) {
                            response_message.push_str(&format!(
                                "- «{} »\n",
                                replacements[i]["value"].as_str().unwrap()
                            ));
                        }
                    }
                }

                response_message.push_str(
                    "\nComme toujours, c'est un plaisir d'assurer la sécurité de la langue.",
                );

                //Send response text
                msg.reply(&ctx, response_message).await.unwrap();
            }
        }
    }

    async fn interaction_create(&self, ctx: Context, interaction: Interaction) {
        //Check if interaction is a command.
        if let Interaction::Command(command) = interaction {
            if command.data.name == "grammar_enable" {
                //Open configuration file.
                let mut user_array: json::Value = json::from_str(
                    &fs::read_to_string("./authorized_users")
                        .await
                        .expect("Could not read users file."),
                )
                .expect("Could not parse users file.");
                //Append user to array.
                user_array
                    .as_array_mut()
                    .unwrap()
                    .push(command.user.id.get().into());
                //Write data to file
                fs::write("./authorized_users", json::to_string(&user_array).unwrap())
                    .await
                    .expect("Could not write into users file.");
                command
                    .create_response(
                        &ctx,
                        CreateInteractionResponse::Message(
                            CreateInteractionResponseMessage::new()
                                .content(":eyes:")
                                .ephemeral(true),
                        ),
                    )
                    .await
                    .unwrap();
            }
            if command.data.name == "grammar_add_word" {
                //TODO: Add word to LanguageTool dictionary.
                command
                    .create_response(
                        &ctx,
                        CreateInteractionResponse::Message(
                            CreateInteractionResponseMessage::new()
                                .content("This function is yet to be implemented.")
                                .ephemeral(true),
                        ),
                    )
                    .await
                    .unwrap();
            }
        }
    }
}

async fn is_willing_user(usr: &User) -> bool {
    //Read config file
    let user_array: json::Value = json::from_str(
        &fs::read_to_string("./authorized_users")
            .await
            .expect("Could not read users file."),
    )
    .expect("Could not parse users file.");
    user_array
        .as_array()
        .unwrap()
        .contains(&usr.id.get().into())
}

#[tokio::main]
async fn main() {
    //Start discord client
    dotenv::dotenv().ok();
    let token = dotenv::var("TOKEN").expect("Could not get TOKEN variable.");
    let intents = GatewayIntents::GUILD_MESSAGES | GatewayIntents::MESSAGE_CONTENT;
    let mut client = Client::builder(&token, intents)
        .event_handler(Handler)
        .await
        .expect("Could not create client");

    if let Err(why) = client.start().await {
        println!("Client error: {why:?}");
    }
}
