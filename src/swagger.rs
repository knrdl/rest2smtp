use std::env;
use std::fs;
use std::io;
use std::path::Path;

pub fn generate_api_doc(api_token_enabled: bool) -> Result<(), io::Error> {
    let api_doc_info = env::var("API_DOC_INFO").unwrap_or("Send mails via REST API".to_string());

    let file_path = Path::new("./www/openapi.yaml");
    let mut content = fs::read_to_string(file_path)?;

    content = content.replace(
        "description: '' # AUTOREPLACED",
        format!("description: '{}'", api_doc_info.replace("'", "\"")).as_str(),
    );

    content = content.replace(
        "security: [] # AUTOREPLACED",
        if api_token_enabled {
            "security:\n        - bearerAuth: []\n        - {}"
        } else {
            ""
        },
    );

    content = content.replace(
        "securitySchemes: {} # AUTOREPLACED",
        if api_token_enabled {
            "securitySchemes:\n    bearerAuth:\n      type: http\n      scheme: bearer"
        } else {
            ""
        },
    );

    fs::write(file_path, content)?;
    Ok(())
}
