use anyhow::{anyhow, Result};
use hyper::body::Incoming;
use std::io::Write;
use std::path::PathBuf;
use std::{env, fs, process};

use http_body_util::BodyExt;
use hyper::{body::Buf, Uri};
use tokio::io::{AsyncRead, AsyncWrite};

use walkdir::WalkDir;

const GO_VERSION_MAJOR: &str = "1.21.0";
// const WASI_SDK_VERSION_MINOR: usize = 0;

async fn tls_connect(url: &Uri) -> Result<impl AsyncRead + AsyncWrite + Unpin> {
    let connector: tokio_native_tls::TlsConnector =
        tokio_native_tls::native_tls::TlsConnector::new()
            .unwrap()
            .into();
    let addr = format!("{}:{}", url.host().unwrap(), url.port_u16().unwrap_or(443));
    let stream = tokio::net::TcpStream::connect(addr).await?;
    let stream = connector.connect(url.host().unwrap(), stream).await?;
    Ok(stream)
}

// Mostly taken from the hyper examples:
// https://github.com/hyperium/hyper/blob/4cf38a12ce7cc5dfd3af356a0cef61ace4ce82b9/examples/client.rs
async fn get_uri(url_str: impl AsRef<str>) -> Result<Incoming> {
    let mut url_string = url_str.as_ref().to_string();
    // This loop will follow redirects and will return when a status code
    // is a success (200-299) or a non-redirect (300-399).
    loop {
        let url: Uri = url_string.parse()?;
        let stream = tls_connect(&url).await?;
        let (mut sender, conn) = hyper::client::conn::http1::handshake(stream).await?;

        tokio::task::spawn(async move {
            if let Err(err) = conn.await {
                println!("Connection failed: {:?}", err);
            }
        });

        let authority = url.authority().unwrap().clone();
        let req = hyper::Request::builder()
            .uri(&url)
            .header(hyper::header::HOST, authority.as_str())
            .body("".to_string())?;

        let res = sender.send_request(req).await?;
        if res.status().is_success() {
            return Ok(res.into_body());
        } else if res.status().is_redirection() {
            let target = res
                .headers()
                .get("Location")
                .ok_or(anyhow!("Redirect without `Location` header"))?;
            url_string = target.to_str()?.to_string();
        } else {
            return Err(anyhow!("Could not request URL {:?}", url));
        }
    }
}

async fn download_go() -> Result<PathBuf> {
    let mut go_dir: PathBuf = env::var("OUT_DIR")?.into();
    go_dir.push("go");

    fs::create_dir_all(&go_dir)?;

    let mut archive_path = go_dir.clone();
    archive_path.push(format!("go.tar.gz"));

    // Download archive if necessary
    if !archive_path.try_exists()? {
        let file_suffix = match (env::consts::OS, env::consts::ARCH) {
            ("linux", "x86") | ("linux", "x86_64") => "linux",
            ("macos", "x86") | ("macos", "x86_64") | ("macos", "aarch64") => ".darwin-amd64",
            ("windows", "x86") => "mingw-x86",
            ("windows", "x86_64") => "mingw",
            other => return Err(anyhow!("Unsupported platform tuple {:?}", other)),
        };

        let uri = format!("https://dl.google.com/go/go{GO_VERSION_MAJOR}{file_suffix}.tar.gz");
        let mut body = get_uri(uri).await?;
        let mut archive = fs::File::create(&archive_path)?;
        while let Some(frame) = body.frame().await {
            if let Some(chunk) = frame
                .map_err(|err| {
                    anyhow!(
                        "Something went wrong when downloading the WASI SDK: {}",
                        err
                    )
                })?
                .data_ref()
            {
                archive.write_all(chunk.chunk())?;
            }
        }
    }

    let mut test_binary = go_dir.clone();
    test_binary.extend(["bin", "go"]);
    // Extract archive if necessary
    if !test_binary.try_exists()? {
        let output = process::Command::new("tar")
            .args([
                "-xf",
                archive_path.to_string_lossy().as_ref(),
                "--strip-components",
                "1",
            ])
            .current_dir(&go_dir)
            .output()?;
        if !output.status.success() {
            return Err(anyhow!(
                "Unpacking WASI SDK failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }
    }

    Ok(go_dir)
}

async fn get_go_path() -> Result<PathBuf> {
    const GO_PATH_ENV_VAR: &str = "JACOBIN_GO_PATH";
    println!("cargo:rerun-if-env-changed={GO_PATH_ENV_VAR}");
    if let Ok(path) = env::var(GO_PATH_ENV_VAR) {
        return Ok(path.into());
    }
    download_go().await
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let go_path = get_go_path().await?;
    if !go_path.try_exists()? {
        return Err(anyhow!(
            "go not installed in specified path of {}",
            go_path.display()
        ));
    }

    // TODO build jacobin
    let output = process::Command::new(format!("{}/bin/go", go_path.display()))
        .env("GOOS", "wasip1")
        .env("GOARCH", "wasm")
        .args(["build", "-o", &format!("{}", env::var("OUT_DIR")?), "./..."])
        .current_dir("jacobin/src")
        .output()?;
    if !output.status.success() {
        return Err(anyhow!(
            "Compaling jacobin failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    // Change the name of the out jacobin to jacobin.wasm
    std::fs::rename(
        format!("{}/jacobin", env::var("OUT_DIR")?),
        format!("{}/jacobin.wasm", env::var("OUT_DIR")?),
    )?;

    for entry in WalkDir::new("jacobin") {
        println!("cargo:rerun-if-changed={}", entry?.path().display());
    }
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=src/lib.rs");
    Ok(())
}
