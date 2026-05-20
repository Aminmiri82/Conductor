use crate::commands::models::{RequestBody, RequestDetail};

pub(super) async fn apply_body(
    mut builder: reqwest::RequestBuilder,
    body: &RequestBody,
) -> Result<reqwest::RequestBuilder, String> {
    match body.mode.as_str() {
        "raw" => {
            builder = builder.body(body.raw.clone());
        }
        "urlencoded" => {
            let pairs = body
                .urlencoded
                .iter()
                .filter(|field| field.enabled && !field.key.is_empty())
                .map(|field| (field.key.clone(), field.value.clone()))
                .collect::<Vec<_>>();
            builder = builder.form(&pairs);
        }
        "formdata" => {
            let mut form = reqwest::multipart::Form::new();
            for field in body
                .form_data
                .iter()
                .filter(|field| field.enabled && !field.key.is_empty())
            {
                if field.field_type == "file" {
                    if let Some(path) = field.file_path.as_ref().filter(|path| !path.is_empty()) {
                        let file_name = std::path::Path::new(path)
                            .file_name()
                            .and_then(|name| name.to_str())
                            .unwrap_or("upload")
                            .to_string();
                        let part = reqwest::multipart::Part::file(path)
                            .await
                            .map_err(|error| error.to_string())?
                            .file_name(file_name);
                        form = form.part(field.key.clone(), part);
                    }
                } else {
                    form = form.text(field.key.clone(), field.value.clone());
                }
            }
            builder = builder.multipart(form);
        }
        _ => {}
    }
    Ok(builder)
}
pub(super) fn normalize_body_files(request: &mut RequestDetail) {
    if let Some(body) = &mut request.body {
        for field in &mut body.form_data {
            if field.field_type == "file" {
                field.file_path = None;
            }
        }
    }
}
