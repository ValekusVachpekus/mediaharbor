use anyhow::Result;
use mediaharbor_core::{ipc, model::Request};

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("--worker") => tokio::runtime::Runtime::new()?.block_on(ipc::worker()),
        Some("--request") => {
            let req: Request = serde_json::from_str(
                args.get(2)
                    .ok_or_else(|| anyhow::anyhow!("Передайте JSON-запрос"))?,
            )?;
            let runtime = tokio::runtime::Runtime::new()?;
            runtime.block_on(ipc::ensure_worker(&std::env::current_exe()?, None))?;
            println!("{}", runtime.block_on(ipc::send(req))?);
            Ok(())
        }
        Some("--register") => {
            let id = args
                .get(2)
                .ok_or_else(|| anyhow::anyhow!("Укажите Chromium extension ID"))?;
            println!(
                "{:?}",
                ipc::register_host(
                    id,
                    "mediaharbor@mediaharbor.local",
                    &std::env::current_exe()?
                )?
            );
            Ok(())
        }
        // Browser passes an origin (Chromium) or host-manifest path (Firefox).
        Some("--native-host") => ipc::native_host(),
        Some("--unregister") => ipc::unregister_host(),
        Some(origin) if origin.starts_with("chrome-extension://") || origin.ends_with(".json") => {
            ipc::native_host()
        }
        _ => {
            println!("MediaHarbor core\n--worker\n--register CHROMIUM_EXTENSION_ID\n--request JSON\n--native-host");
            Ok(())
        }
    }
}
