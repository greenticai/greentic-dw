use std::io::{self, Write};

#[derive(Clone, Copy)]
pub(crate) enum MsgKey {
    ManifestId,
    DisplayName,
    Tenant,
    Team,
    RequestedLocale,
    HumanLocale,
}

pub(crate) fn prompt(locale: &str, key: MsgKey) -> Result<String, io::Error> {
    let mut stderr = io::stderr().lock();
    write!(stderr, "{}", localized(locale, key))?;
    stderr.flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    Ok(input.trim().to_string())
}

pub(crate) fn localized(locale: &str, key: MsgKey) -> &'static str {
    let lang = locale.split(['-', '_']).next().unwrap_or("en");

    match (lang, key) {
        ("nl", MsgKey::ManifestId) => "Manifest-ID invoeren: ",
        ("nl", MsgKey::DisplayName) => "Weergavenaam invoeren: ",
        ("nl", MsgKey::Tenant) => "Tenant invoeren: ",
        ("nl", MsgKey::Team) => "Team invoeren (optioneel): ",
        ("nl", MsgKey::RequestedLocale) => "Gevraagde locale invoeren (optioneel): ",
        ("nl", MsgKey::HumanLocale) => "Human locale invoeren (optioneel): ",
        (_, MsgKey::ManifestId) => "Enter manifest id: ",
        (_, MsgKey::DisplayName) => "Enter display name: ",
        (_, MsgKey::Tenant) => "Enter tenant: ",
        (_, MsgKey::Team) => "Enter team (optional): ",
        (_, MsgKey::RequestedLocale) => "Enter requested locale (optional): ",
        (_, MsgKey::HumanLocale) => "Enter human locale (optional): ",
    }
}
