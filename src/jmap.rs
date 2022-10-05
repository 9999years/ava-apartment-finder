use chrono::TimeZone;
use chrono::Utc;
use color_eyre::eyre;
use color_eyre::eyre::eyre;
use color_eyre::eyre::Context;
use jmap_client::client::Client;
use jmap_client::client::Credentials;
use jmap_client::core::query::Comparator;
use jmap_client::core::query::Filter;
use jmap_client::email::EmailAddress;
use jmap_client::identity::Property as IdentityProperty;
use jmap_client::mailbox::query::Comparator as MailboxComparator;
use jmap_client::mailbox::query::Filter as MailboxFilter;
use jmap_client::mailbox::Property as MailboxProperty;
use jmap_client::mailbox::Role;

const API_ENDPOINT: &str = "https://api.fastmail.com/jmap/session";

#[derive(Debug)]
pub struct Email {
    pub to: EmailAddress,
    pub from: EmailAddress,
    pub subject: String,
    pub body: String,
}

impl Email {
    #[tracing::instrument]
    pub async fn send(&self) -> eyre::Result<()> {
        let bearer_token =
            std::env::var("FASTMAIL_API_TOKEN").wrap_err("Couldn't get $FASTMAIL_API_TOKEN")?;

        let client = Client::new()
            .credentials(Credentials::Bearer(bearer_token))
            .connect(API_ENDPOINT)
            .await
            .map_err(|err| eyre!("{err}"))
            .wrap_err("Failed to connect to server")?;

        tracing::debug!("Email client initialized");

        let mailbox_filter: Option<Filter<MailboxFilter>> = None;
        let mailbox_sort: Option<Vec<Comparator<MailboxComparator>>> = None;
        let mailboxes = client
            .mailbox_query(mailbox_filter, mailbox_sort)
            .await
            .map_err(|err| eyre!("{err}"))?;

        let mut mailbox_id = None;

        for id in mailboxes.ids() {
            let mailbox = client
                .mailbox_get(
                    id,
                    Some(vec![
                        MailboxProperty::Name,
                        MailboxProperty::ParentId,
                        MailboxProperty::Role,
                    ]),
                )
                .await
                .map_err(|err| eyre!("{err}"))?
                .ok_or_else(|| eyre!("Unable to find mailbox {id}"))?;

            if let Role::Inbox = mailbox.role() {
                mailbox_id = Some(id);
            }
        }

        let mailbox_id = mailbox_id.ok_or_else(|| eyre!("Unable to find Inbox ID"))?;

        tracing::debug!("Using mailbox ID {mailbox_id}");

        let identities = client
            .identity_get(
                None,
                Some(vec![
                    IdentityProperty::Id,
                    IdentityProperty::Name,
                    IdentityProperty::Email,
                    IdentityProperty::ReplyTo,
                ]),
            )
            .await
            .map_err(|err| eyre!("{err}"))?;

        let mut identity = None;
        for ident in identities {
            if ident.email() == Some(self.from.email()) && self.from.name() == ident.name() {
                identity = Some(ident);
            }
        }
        let identity = identity.ok_or_else(|| {
            eyre!(
                "Unable to find sending identity for email {}",
                self.from.email()
            )
        })?;
        let identity_id = identity
            .id()
            .ok_or_else(|| eyre!("Identity has no ID: {identity:?}"))?;

        let keywords: Option<Vec<&'static str>> = None;

        let email = client
            .email_import(
                format!(
                    "To: {}\r\n\
                    From: {}\r\n\
                    Subject: {}\r\n\
                    \r\n\
                    {}\r\n",
                    self.to,
                    self.from,
                    self.subject,
                    self.body.to_string().replace('\n', "\r\n")
                )
                .as_bytes()
                .to_vec(),
                vec![mailbox_id],
                keywords,
                None,
            )
            .await
            .map_err(|err| eyre!("{err}"))
            .wrap_err("Failed to import email")?;

        let email_id = email
            .id()
            .ok_or_else(|| eyre!("Imported email has no ID"))?;

        tracing::debug!(id = email_id, "Imported email");

        let submission = client
            .email_submission_create(email_id, identity_id)
            .await
            .map_err(|err| eyre!("{err}"))
            .wrap_err("Failed to send email")?;

        tracing::info!(
            to = %self.to,
            subject=%self.subject,
            send_at = %submission.send_at().map(|i| Utc.timestamp(i, 0)).unwrap_or_default(),
            "Sent email!"
        );

        Ok(())
    }
}
