#[cfg(not(feature = "desktop"))]
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut instance = None;
    let mut set_password = false;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--instance" => {
                if instance.is_some() {
                    anyhow::bail!("--instance may only be specified once");
                }
                let id = args
                    .next()
                    .filter(|value| !value.starts_with("--"))
                    .ok_or_else(|| anyhow::anyhow!("--instance requires an identifier"))?;
                instance = Some(id);
            }
            "--set-password" => {
                if set_password {
                    anyhow::bail!("--set-password may only be specified once");
                }
                set_password = true;
            }
            _ => anyhow::bail!("Unknown argument: {arg}"),
        }
    }

    tuicommander_lib::app_instance::select_app_instance(instance.as_deref())
        .map_err(anyhow::Error::msg)?;

    if set_password {
        return tuicommander_lib::set_password_interactive();
    }

    let port: u16 = match std::env::var("TUIC_PORT") {
        Ok(val) => val.parse().unwrap_or_else(|e| {
            eprintln!("warning: TUIC_PORT={val:?} is not a valid port ({e}), using default 9877");
            9877
        }),
        Err(_) => 9877,
    };

    tuicommander_lib::run_remote(port).await
}

#[cfg(feature = "desktop")]
fn main() {
    eprintln!("tuic-remote requires --no-default-features (desktop feature must be disabled)");
    std::process::exit(1);
}
