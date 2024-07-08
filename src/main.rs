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

        let _ = model::application::Command::create_global_command(&ctx, builder).await;

        let builder = CreateCommand::new("grammar_add_word")
            .description("Add word to dictionary.")
            .add_option(CreateCommandOption::new(
                serenity::all::CommandOptionType::String,
                "word",
                "Word to add to dictionary.",
            ));

        let _ = model::application::Command::create_global_command(&ctx, builder).await;
    }

    async fn message(&self, ctx: Context, msg: Message) {
        //Only correct authorised users.
        if match is_willing_user(&msg.author).await {
            Ok(x) => x,
            Err(e) => {
                eprintln!("{e}");
                return;
            }
        } {
            //Message to be sent to the user
            let mut response_message = String::new();

            //Make http request to local LanguageTools server and parse it to json.
            let response = match match reqwest::get(format!(
                "http://localhost:8081/v2/check?language={}&text={}",
                "fr-CA", msg.content
            ))
            .await
            {
                Ok(x) => x,
                Err(e) => {
                    eprintln!("LanguageTool request failed. {e}");
                    return;
                }
            }
            .text()
            .await
            {
                Ok(x) => x,
                Err(e) => {
                    eprintln!("{e}");
                    return;
                }
            };

            let response: json::Value = match json::from_str(&response) {
                Ok(x) => x,
                Err(e) => {
                    eprintln!("Could not parse LanguageTool response as json. {e}");
                    return;
                }
            };

            let mut grammar_matches: Vec<json::Value> = Vec::new();
            let mut spelling_matches: Vec<json::Value> = Vec::new();

            let matches = response["matches"].as_array().unwrap();
            //Seperate grammar and spelling mistakes.
            for mistake in matches {
                if mistake["rule"]["issueType"] != "misspelling" {
                    //Filter grammar matches.
                    //Tolerate french Canadian punctuation.
                    if mistake["message"] == "Les deux-points sont précédés d’une espace insécable."
                    {
                        continue;
                    }
                    grammar_matches.push(mistake.clone());
                } else {
                    //Filter spelling mistakes.
                    let context = mistake["context"]["text"].as_str().unwrap();
                    let index = mistake["context"]["offset"].as_u64().unwrap() as usize;
                    let length = mistake["context"]["length"].as_u64().unwrap() as usize;
                    let mut word = context[index..(index + length)].to_string();

                    //Remove period.
                    let period_index = word.find(".");
                    if let Some(u) = period_index {
                        word.remove(u);
                    }

                    //Ignore proper nouns.
                    if word.starts_with(|c: char| c.is_uppercase()) {
                        continue;
                    }
                    //Ignore words that are surrounded by quotes or asterisks.
                    //French quotes are not considered because they would be a pain, considering they
                    //need to be preceeded and followed by spaces. That being said, borrowed words
                    //should be italicized anyway.
                    let preceeding_char = context.chars().nth(index - 1);
                    let following_char = context.chars().nth(index + length);
                    if preceeding_char.is_some() && following_char.is_some() {
                        if (preceeding_char.unwrap() == '"' && following_char.unwrap() == '"')
                            || (preceeding_char.unwrap() == '*' && following_char.unwrap() == '*')
                        {
                            continue;
                        }
                    }

                    //Add the filtered mistake to spelling_matches.
                    spelling_matches.push(mistake.clone());
                }
            }

            //Only do stuff if mistakes were found.
            if grammar_matches.len() + spelling_matches.len() != 0 {
                //Manage matches and generate response text.
                response_message.push_str("Halte-là !\n\n");

                //Generate grammar corrections
                for mistake in grammar_matches {
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
                                "- « {} »\n",
                                replacements[i]["value"].as_str().unwrap()
                            ));
                        }
                    }
                }

                //Generate spelling corrections
                for mistake in spelling_matches {
                    response_message.push_str(&format!(
                        "Le mot « {} » n'est pas reconnu.\n",
                        mistake["context"]["text"].as_str().unwrap()[(mistake["context"]["offset"]
                            .as_u64()
                            .unwrap()
                            as usize)
                            ..((mistake["context"]["offset"].as_u64().unwrap()
                                + mistake["context"]["length"].as_u64().unwrap())
                                as usize)]
                            .to_string()
                    ));

                    let replacements = mistake["replacements"].as_array().unwrap();
                    if replacements.len() != 0 {
                        response_message.push_str("Voici des corrections possibles:\n");
                        for i in 0..3.min(replacements.len()) {
                            response_message.push_str(&format!(
                                "- « {} »\n",
                                replacements[i]["value"].as_str().unwrap()
                            ));
                        }
                    }
                }

                response_message.push_str(
                    "\nComme toujours, c'est un plaisir d'assurer la sécurité de la langue.",
                );

                //Send response text
                if let Err(e) = msg.reply(&ctx, response_message).await {
                    eprintln!("Could not send discord message.{e}");
                };
            }
        }
    }

    async fn interaction_create(&self, ctx: Context, interaction: Interaction) {
        //Check if interaction is a command.
        if let Interaction::Command(command) = interaction {
            if command.data.name == "grammar_enable" {
                //Open configuration file.
                let user_array = match fs::read_to_string("./authorized_users").await {
                    Ok(x) => x,
                    Err(e) => {
                        eprintln!("Could not read users file. {e}");
                        return;
                    }
                };
                let mut user_array: json::Value = match json::from_str(&user_array) {
                    Ok(x) => x,
                    Err(e) => {
                        eprintln!("Could not parse users file. {e}");
                        return;
                    }
                };
                //Append user to array.
                match user_array.as_array_mut() {
                    Some(x) => x,
                    None => return,
                }
                .push(command.user.id.get().into());
                //Write data to file
                match fs::write(
                    "./authorized_users",
                    match json::to_string(&user_array) {
                        Ok(x) => x,
                        Err(_) => return,
                    },
                )
                .await
                {
                    Ok(x) => x,
                    Err(e) => {
                        eprintln!("Could not write into users file. {e}");
                        return;
                    }
                }
                let _ = command
                    .create_response(
                        &ctx,
                        CreateInteractionResponse::Message(
                            CreateInteractionResponseMessage::new()
                                .content(":eyes:")
                                .ephemeral(true),
                        ),
                    )
                    .await;
            }
            if command.data.name == "grammar_add_word" {
                //TODO: Add word to LanguageTool dictionary.
                let _ = command
                    .create_response(
                        &ctx,
                        CreateInteractionResponse::Message(
                            CreateInteractionResponseMessage::new()
                                .content("This function is yet to be implemented.")
                                .ephemeral(true),
                        ),
                    )
                    .await;
            }
        }
    }
}

async fn is_willing_user(usr: &User) -> Result<bool, std::io::Error> {
    //Read config file
    let user_array: json::Value = json::from_str(&fs::read_to_string("./authorized_users").await?)?;
    Ok(match user_array.as_array() {
        Some(x) => x,
        //Default to returning false if there was a problem with reading the user array.
        None => {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "The user file does not contain a json array.",
            ))
        }
    }
    .contains(&usr.id.get().into()))
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
        eprintln!("Client error: {why:?}");
    }
}
