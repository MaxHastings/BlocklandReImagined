//! Package distribution over the game transport. A client opens a download
//! connection (`Purpose::Download`) before joining, asks for the server's
//! environment, and fetches every shared or client package it lacks into its
//! content-addressed cache (`bri_package::sync`). The server offers only
//! packages it loads on the client side; server-only packages and any other
//! file are never reachable. Everything received is verified before use.
use crate::{codec, protocol::*};
use anyhow::{Context, Result, bail, ensure};
use bri_package::{
    environment::{Environment, PackageRef},
    packages::{PackageSet, package_dir},
    sync::{Cache, Listing},
};
use bri_progress::{Progress, Stage, Unit};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

/// Most a client downloads for one server before asking its player: a
/// hostile server must not be able to fill a disk with valid packages.
pub const MAX_FETCH_BYTES: u64 = 4 * 1024 * 1024 * 1024;
/// A download connection that sends no request for this long is closed.
pub const DOWNLOAD_IDLE: Duration = Duration::from_secs(15);

/// What a server offers for download: listings of its shared and client
/// packages, and where each of their files lives.
pub struct PackageShelf {
    offered: Vec<PackageRef>,
    listings: BTreeMap<String, Listing>,
    objects: BTreeMap<String, Offered>,
}

/// Where an offered file lives and what it looked like when it was listed.
struct Offered {
    package: String,
    path: PathBuf,
    stamp: Option<(u64, std::time::SystemTime)>,
}

fn stamp(path: &Path) -> Option<(u64, std::time::SystemTime)> {
    let meta = std::fs::metadata(path).ok()?;
    Some((meta.len(), meta.modified().ok()?))
}

impl PackageShelf {
    /// List every client-side package of `environment` (loaded from `set`
    /// under `root`). Fails if a package cannot be sent, naming it.
    pub fn new(root: &Path, set: &PackageSet, environment: &Environment) -> Result<Self> {
        let mut shelf = Self {
            offered: environment.client_packages(),
            listings: BTreeMap::new(),
            objects: BTreeMap::new(),
        };
        for package in &shelf.offered {
            let entry = set
                .packages
                .iter()
                .find(|e| e.id == package.id)
                .with_context(|| format!("Package `{}` is not in the package list", package.id))?;
            let dir = package_dir(root, entry)?;
            let listing = Listing::of(&dir, package)?;
            for file in &listing.files {
                let path = dir.join(&file.path);
                shelf
                    .objects
                    .entry(file.sha256.clone())
                    .or_insert_with(|| Offered {
                        package: package.id.clone(),
                        stamp: stamp(&path),
                        path,
                    });
            }
            shelf.listings.insert(package.hash.clone(), listing);
        }
        Ok(shelf)
    }

    fn answer(&self, request: DownloadRequest) -> DownloadReply {
        match request {
            DownloadRequest::Environment => DownloadReply::Environment(self.offered.clone()),
            DownloadRequest::Listing { hash } => match self.listings.get(&hash) {
                Some(listing) => DownloadReply::Listing(Box::new(listing.clone())),
                None => DownloadReply::Refused("This server does not offer that package".into()),
            },
            DownloadRequest::Object {
                sha256,
                offset,
                length,
            } => {
                let Some(offered) = self.objects.get(&sha256) else {
                    return DownloadReply::Refused("This server does not offer that file".into());
                };
                // Listed once at startup; a file edited since would reach
                // the client as bytes that fail their hash, which reads as
                // a hostile server. Say what actually happened instead.
                let now = stamp(&offered.path);
                if now.is_none() || now != offered.stamp {
                    return DownloadReply::Refused(format!(
                        "Package `{}` changed on the host after it was loaded; the host must restart to offer it",
                        offered.package
                    ));
                }
                let (path, size) = (&offered.path, now.map_or(0, |(size, _)| size));
                if length == 0
                    || length > MAX_OBJECT_CHUNK
                    || offset.saturating_add(length.into()) > size
                {
                    return DownloadReply::Refused("Invalid file range".into());
                }
                match bri_package::sync::read_range(path, offset, length as usize) {
                    Ok(bytes) => DownloadReply::Object(bytes),
                    Err(error) => {
                        DownloadReply::Refused(format!("Could not read the file: {error}"))
                    }
                }
            }
        }
    }
}

/// Serve one download connection until the client finishes or goes idle.
pub(crate) async fn serve(
    shelf: Arc<PackageShelf>,
    send: &mut quinn::SendStream,
    receive: &mut quinn::RecvStream,
) -> Result<()> {
    loop {
        let request =
            match tokio::time::timeout(DOWNLOAD_IDLE, codec::read_small_request(receive)).await {
                Ok(Ok(request)) => request,
                // Idle, finished or malformed: the connection ends either way.
                _ => return Ok(()),
            };
        let shelf = shelf.clone();
        let reply = tokio::task::spawn_blocking(move || shelf.answer(request)).await?;
        let refused = matches!(reply, DownloadReply::Refused(_));
        tokio::time::timeout(
            DOWNLOAD_IDLE,
            codec::write_frame(send, &codec::encode(&reply)?),
        )
        .await??;
        if refused {
            // A client that asks for what is not offered is done.
            send.finish()?;
            return Ok(());
        }
    }
}

/// One package a fetch made available.
#[derive(Debug, Clone)]
pub struct Fetched {
    pub package: PackageRef,
    pub dir: PathBuf,
    /// Bytes this fetch downloaded for it (0 when the cache already held it).
    pub downloaded: u64,
}

/// Fetch every shared and client package the server at `address` offers
/// that `cache` does not hold, verify and install them, and return where
/// each offered package now lives. Reports bytes into `progress` under
/// [`Stage::DownloadingPackages`].
pub async fn fetch_missing(
    address: std::net::SocketAddr,
    certificate: &[u8],
    cache: &Cache,
    progress: &Progress,
) -> Result<Vec<Fetched>> {
    let (endpoint, connection) = crate::client::open(address, certificate).await?;
    let result = async {
        let (mut send, mut receive) = connection.open_bi().await?;
        codec::write_small_request(
            &mut send,
            &JoinBegin {
                version: VERSION,
                purpose: Purpose::Download,
            },
        )
        .await?;
        let mut ask = async |request: DownloadRequest| -> Result<DownloadReply> {
            codec::write_small_request(&mut send, &request).await?;
            let frame = tokio::time::timeout(
                DOWNLOAD_IDLE,
                codec::read_frame(&mut receive, codec::MAX_FRAME),
            )
            .await
            .context("The server stopped answering the download")??;
            match codec::decode::<DownloadReply>(&frame) {
                Ok(DownloadReply::Refused(reason)) => {
                    bail!("Server refused the download: {reason}")
                }
                Ok(reply) => Ok(reply),
                Err(_) => match codec::decode::<Message>(&frame) {
                    Ok(Message::Rejected(reason)) => bail!("Server refused the download: {reason}"),
                    _ => bail!("Malformed download reply"),
                },
            }
        };
        let DownloadReply::Environment(offered) = ask(DownloadRequest::Environment).await? else {
            bail!("Expected the server's package list");
        };
        Environment::validate_refs(&offered)?;
        let mut listings = Vec::new();
        for package in &offered {
            if cache.installed(package).is_some() {
                continue;
            }
            let DownloadReply::Listing(listing) = ask(DownloadRequest::Listing {
                hash: package.hash.clone(),
            })
            .await?
            else {
                bail!("Expected the file list of {package}");
            };
            listing
                .validate(package)
                .with_context(|| format!("Package {package} from the server is unsafe"))?;
            listings.push(*listing);
        }
        // Objects are shared between packages; each downloads once.
        let total: u64 = listings
            .iter()
            .flat_map(|l| cache.missing(l))
            .map(|f| (&f.sha256, f.size))
            .collect::<BTreeMap<_, _>>()
            .values()
            .sum();
        ensure!(
            total <= MAX_FETCH_BYTES,
            "The server's packages need {} MB of downloads, over the {} MB limit",
            total / (1024 * 1024),
            MAX_FETCH_BYTES / (1024 * 1024)
        );
        progress.begin(Stage::DownloadingPackages, Unit::Bytes, Some(total));
        let mut done = 0_u64;
        let mut downloaded = BTreeMap::<String, u64>::new();
        for listing in &listings {
            for file in cache.missing(listing) {
                let mut writer = cache.receive(file)?;
                while writer.remaining() > 0 {
                    let length = writer.remaining().min(MAX_OBJECT_CHUNK.into()) as u32;
                    let DownloadReply::Object(bytes) = ask(DownloadRequest::Object {
                        sha256: file.sha256.clone(),
                        offset: writer.written(),
                        length,
                    })
                    .await?
                    else {
                        bail!("Expected bytes of {}", file.path);
                    };
                    ensure!(
                        bytes.len() == length as usize,
                        "{}: the server sent the wrong range",
                        file.path
                    );
                    writer.write(&bytes).with_context(|| {
                        format!("Package {} file {}", listing.package, file.path)
                    })?;
                    done += bytes.len() as u64;
                    progress.set_in(Stage::DownloadingPackages, done);
                }
                writer
                    .finish()
                    .with_context(|| format!("Package {}", listing.package))?;
                *downloaded.entry(listing.package.hash.clone()).or_default() += file.size;
            }
        }
        // Keep the cache bounded across every server this client visits;
        // what this server needs stays. Failing to prune is not a failed
        // fetch.
        let _ = cache.prune(bri_package::sync::CACHE_BYTES, &offered);
        let mut fetched = Vec::new();
        for package in offered {
            let dir = match listings.iter().find(|l| l.package == package) {
                Some(listing) => cache
                    .install(listing)
                    .with_context(|| format!("Installing package {package}"))?,
                None => cache
                    .installed(&package)
                    .context("Cached package disappeared")?,
            };
            fetched.push(Fetched {
                downloaded: downloaded.get(&package.hash).copied().unwrap_or(0),
                package,
                dir,
            });
        }
        let _ = send.finish();
        Ok(fetched)
    }
    .await;
    connection.close(0_u32.into(), b"Download complete");
    drop(endpoint);
    result
}
