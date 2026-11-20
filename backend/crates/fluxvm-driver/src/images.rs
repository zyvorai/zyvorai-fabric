// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

//! `ImageDriver` backed by FluxVM's image catalog
//! (`GET/POST/DELETE /v1/images/catalog...`) rather than machinectl's
//! `/var/lib/machines` image directory. The two models don't map 1:1 — see
//! the per-method notes below for what's a real equivalent versus a clear
//! "not supported" error.

use anyhow::{bail, Result};
use async_trait::async_trait;
use zyvor_fabric_driver_core::{ImageDriver, ImageInfo};

use crate::FluxVmDriver;

fn unsupported(op: &str, why: &str) -> anyhow::Error {
    anyhow::anyhow!("'{op}' is not supported by the fluxvm backend — {why}")
}

#[async_trait]
impl ImageDriver for FluxVmDriver {
    async fn list_images(&self) -> Result<Vec<ImageInfo>> {
        let entries = self.client.list_catalog().await?;
        Ok(entries
            .into_iter()
            .map(|e| ImageInfo {
                name: e.name,
                image_type: e.format,
                read_only: e.read_only,
                // No cheap way to report a size for a remote catalog entry
                // (it may not even be fetched to this host yet) — matches
                // machinectl's own loosely-typed `size: String` by leaving
                // it blank rather than guessing.
                size: String::new(),
            })
            .collect())
    }

    async fn clone_image(&self, source: &str, target: &str) -> Result<()> {
        self.client
            .clone_catalog_entry(source, target)
            .await
            .map(|_| ())
    }

    async fn rename_image(&self, old_name: &str, new_name: &str) -> Result<()> {
        self.client
            .rename_catalog_entry(old_name, new_name)
            .await
            .map(|_| ())
    }

    async fn remove_image(&self, name: &str) -> Result<()> {
        self.client.remove_catalog_entry(name).await
    }

    async fn set_image_read_only(&self, name: &str, read_only: bool) -> Result<()> {
        self.client
            .set_catalog_read_only(name, read_only)
            .await
            .map(|_| ())
    }

    async fn pull_raw_image(&self, url: &str, name: &str, verify: bool) -> Result<()> {
        if verify {
            bail!(unsupported(
                "pull_raw_image(verify=true)",
                "catalog signature verification is a separate offline flow (`fluxvm catalog sign` \
                 at authoring time, automatic re-verification at VM-create time when trusted_signers \
                 is configured) — there's no per-request \"verify now\" equivalent",
            ));
        }
        self.client
            .add_catalog_entry(name, url, "raw")
            .await
            .map(|_| ())
    }

    async fn pull_tar_image(&self, _url: &str, _name: &str, _verify: bool) -> Result<()> {
        Err(unsupported(
            "pull_tar_image",
            "tar/directory-tree images have no QEMU/Cloud-Hypervisor/Firecracker equivalent — \
             those boot disk images, not container rootfs trees",
        ))
    }

    async fn import_raw_image(&self, path: &str, name: &str) -> Result<()> {
        self.client
            .add_catalog_entry(name, path, "raw")
            .await
            .map(|_| ())
    }

    async fn import_tar_image(&self, _path: &str, _name: &str) -> Result<()> {
        Err(unsupported(
            "import_tar_image",
            "tar/directory-tree images have no QEMU/Cloud-Hypervisor/Firecracker equivalent",
        ))
    }

    async fn export_raw_image(&self, name: &str, path: &str) -> Result<()> {
        self.client
            .export_catalog_entry(name, std::path::Path::new(path))
            .await
    }

    async fn export_tar_image(&self, _name: &str, _path: &str) -> Result<()> {
        Err(unsupported(
            "export_tar_image",
            "tar/directory-tree images have no QEMU/Cloud-Hypervisor/Firecracker equivalent",
        ))
    }

    async fn clean_images(&self, _all: bool) -> Result<()> {
        // machinectl's "hidden/cached image" concept doesn't map onto
        // catalog entries themselves, but the catalog does accumulate one
        // real analog: cached URL downloads no longer referenced by any
        // entry (e.g. after a rename or remove). Clean those.
        let removed = self.client.clean_catalog().await?;
        tracing::info!(
            "fluxvm clean_images: removed {} orphaned download(s)",
            removed.len()
        );
        Ok(())
    }
}
