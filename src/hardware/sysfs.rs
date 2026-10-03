//! Linux storage-device classification using `/sys`.
//!
//! The crate deliberately avoids `udev` and other runtime dependencies.  It uses
//! the Linux sysfs block-device interface to classify physical storage devices.
//!
//! Typical results:
//! - NVMe SSD -> [`StorageDeviceType::Nvme`]
//! - USB thumb/flash drive -> [`StorageDeviceType::UsbFlashDrive`]
//! - Internal SATA/SCSI HDD -> [`StorageDeviceType::Hdd`]
//! - USB-attached spinning HDD -> [`StorageDeviceType::ExternalHdd`]
//!
//! A partition such as `/dev/sda1` is automatically resolved to its parent disk
//! (`/dev/sda`) before classification.

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[cfg(target_os = "linux")]
use std::os::unix::fs::FileTypeExt;

/// High-level storage-device class.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StorageDeviceType {
    /// NVMe block device, normally an NVMe SSD.
    Nvme,
    /// Removable USB storage such as a thumb/flash drive.
    UsbFlashDrive,
    /// Internal or non-USB rotating hard disk.
    Hdd,
    /// Rotating hard disk attached through USB.
    ExternalHdd,
    /// Non-NVMe SSD attached through a non-USB interface.
    Ssd,
    /// Non-rotating USB storage that does not look removable enough to call a flash drive.
    ExternalSsd,
    /// A physical block device was found but its type could not be determined.
    Other,
}

impl fmt::Display for StorageDeviceType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            Self::Nvme => "nvme",
            Self::UsbFlashDrive => "usb-flash-drive",
            Self::Hdd => "hdd",
            Self::ExternalHdd => "external-hdd",
            Self::Ssd => "ssd",
            Self::ExternalSsd => "external-ssd",
            Self::Other => "other",
        };
        f.write_str(value)
    }
}

/// The transport/bus family detected for a device.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BusType {
    Nvme,
    Usb,
    Sata,
    Scsi,
    Mmio,
    Other,
    Unknown,
}

impl fmt::Display for BusType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            Self::Nvme => "nvme",
            Self::Usb => "usb",
            Self::Sata => "sata",
            Self::Scsi => "scsi",
            Self::Mmio => "mmio",
            Self::Other => "other",
            Self::Unknown => "unknown",
        };
        f.write_str(value)
    }
}

/// Information collected from Linux sysfs for a physical block device.
#[derive(Debug, Clone)]
pub struct StorageDevice {
    /// The original path supplied to [`identify`].
    pub _requested_path: PathBuf,
    /// Physical whole-disk node used for classification, e.g. `/dev/sda`.
    pub device_path: PathBuf,
    /// Sysfs directory for the whole-disk block device.
    pub sysfs_path: PathBuf,
    /// Kernel block-device name, e.g. `sda` or `nvme0n1`.
    pub name: String,
    /// Human-readable model, when exposed by sysfs.
    pub model: Option<String>,
    /// Vendor/manufacturer, when exposed by sysfs.
    pub vendor: Option<String>,
    /// Detected bus/transport.
    pub bus: BusType,
    /// Linux removable flag.
    pub removable: bool,
    /// Linux rotational flag.
    pub rotational: bool,
    /// Final high-level classification.
    pub kind: StorageDeviceType,
}

#[derive(Debug)]
pub enum IdentifyError {
    NotLinux,
    InvalidDevicePath(PathBuf),
    NotBlockDevice(PathBuf),
    UnsupportedVirtualDevice(PathBuf),
    Io { path: PathBuf, source: io::Error },
    InvalidSysfsValue { path: PathBuf, value: String },
}

impl fmt::Display for IdentifyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotLinux => f.write_str("storage-device-type currently supports Linux only"),
            Self::InvalidDevicePath(path) => write!(f, "not a valid /dev device path: {}", path.display()),
            Self::NotBlockDevice(path) => write!(f, "not a block device: {}", path.display()),
            Self::UnsupportedVirtualDevice(path) => {
                write!(f, "virtual/device-mapper block device is not a physical disk: {}", path.display())
            }
            Self::Io { path, source } => write!(f, "failed to read {}: {}", path.display(), source),
            Self::InvalidSysfsValue { path, value } => {
                write!(f, "invalid sysfs value at {}: {:?}", path.display(), value)
            }
        }
    }
}

impl std::error::Error for IdentifyError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

/// Identify a Linux storage device from a device node such as `/dev/sda`, `/dev/nvme0n1`,
/// or a partition such as `/dev/sda1`.
///
/// Symlinked device paths (`/dev/disk/by-id/...`) are accepted as long as they resolve
/// to a physical block device.
#[cfg(target_os = "linux")]
pub fn identify<P: AsRef<Path>>(path: P) -> Result<StorageDevice, IdentifyError> {
    let requested_path = path.as_ref().to_path_buf();
    let canonical = fs::canonicalize(&requested_path).map_err(|source| IdentifyError::Io {
        path: requested_path.clone(),
        source,
    })?;

    let metadata = fs::metadata(&canonical).map_err(|source| IdentifyError::Io {
        path: canonical.clone(),
        source,
    })?;

    if !metadata.file_type().is_block_device() {
        return Err(IdentifyError::NotBlockDevice(canonical));
    }

    let name = canonical
        .file_name()
        .and_then(|x| x.to_str())
        .ok_or_else(|| IdentifyError::InvalidDevicePath(canonical.clone()))?
        .to_owned();

    reject_virtual_devices(&name, &canonical)?;

    let class_link = PathBuf::from("/sys/class/block").join(&name);
    if !class_link.exists() {
        return Err(IdentifyError::Io {
            path: class_link,
            source: io::Error::new(io::ErrorKind::NotFound, "sysfs block-device entry not found"),
        });
    }

    let sysfs_path = resolve_whole_disk_sysfs(&class_link, &name)?;
    let disk_name = sysfs_path
        .file_name()
        .and_then(|x| x.to_str())
        .ok_or_else(|| IdentifyError::InvalidDevicePath(sysfs_path.clone()))?
        .to_owned();

    let device_path = PathBuf::from("/dev").join(&disk_name);
    let rotational = read_flag(&sysfs_path.join("queue/rotational"))?;
    let removable = read_flag(&sysfs_path.join("removable"))?;
    let model = read_trimmed_optional(&first_existing(&[
        sysfs_path.join("device/model"),
        sysfs_path.join("device/name"),
    ]));
    let vendor = read_trimmed_optional(&first_existing(&[
        sysfs_path.join("device/vendor"),
        sysfs_path.join("device/manufacturer"),
    ]));

    let bus = detect_bus(&sysfs_path);
    let kind = classify(disk_name.as_str(), bus, rotational, removable);

    Ok(StorageDevice {
        _requested_path:requested_path,
        device_path,
        sysfs_path,
        name: disk_name,
        model,
        vendor,
        bus,
        removable,
        rotational,
        kind,
    })
}

/// Stub returned when compiling on a non-Linux host.
#[cfg(not(target_os = "linux"))]
pub fn identify<P: AsRef<Path>>(_path: P) -> Result<StorageDevice, IdentifyError> {
    Err(IdentifyError::NotLinux)
}

#[cfg(target_os = "linux")]
fn reject_virtual_devices(name: &str, path: &Path) -> Result<(), IdentifyError> {
    // Device-mapper (/dev/dm-*) and common MD/LVM virtual devices do not represent
    // a single physical storage device and therefore should not be mislabeled as HDD/SSD.
    let virtual_name = name.starts_with("dm-")
        || name.starts_with("md")
        || name.starts_with("loop")
        || name.starts_with("zram");
    if virtual_name {
        Err(IdentifyError::UnsupportedVirtualDevice(path.to_path_buf()))
    } else {
        Ok(())
    }
}

#[cfg(target_os = "linux")]
fn resolve_whole_disk_sysfs(class_link: &Path, name: &str) -> Result<PathBuf, IdentifyError> {
    let canonical = fs::canonicalize(class_link).map_err(|source| IdentifyError::Io {
        path: class_link.to_path_buf(),
        source,
    })?;

    // A partition has a `partition` attribute and normally resolves to:
    // .../block/sda/sda1 or .../nvme0n1/nvme0n1p1. Its direct parent is the disk.
    if class_link.join("partition").exists() {
        let parent = canonical.parent().ok_or_else(|| IdentifyError::InvalidDevicePath(canonical.clone()))?;
        let parent_name = parent
            .file_name()
            .and_then(|x| x.to_str())
            .ok_or_else(|| IdentifyError::InvalidDevicePath(parent.to_path_buf()))?;
        if parent_name == name {
            return Ok(canonical);
        }
        return Ok(parent.to_path_buf());
    }

    Ok(canonical)
}

#[cfg(target_os = "linux")]
fn read_flag(path: &Path) -> Result<bool, IdentifyError> {
    let raw = fs::read_to_string(path).map_err(|source| IdentifyError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    match raw.trim() {
        "0" => Ok(false),
        "1" => Ok(true),
        value => Err(IdentifyError::InvalidSysfsValue {
            path: path.to_path_buf(),
            value: value.to_owned(),
        }),
    }
}

#[cfg(target_os = "linux")]
fn read_trimmed_optional(path: &Path) -> Option<String> {
    fs::read_to_string(path)
        .ok()
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
}

#[cfg(target_os = "linux")]
fn first_existing(paths: &[PathBuf]) -> PathBuf {
    paths
        .iter()
        .find(|p| p.exists())
        .cloned()
        .unwrap_or_default()
}

#[cfg(target_os = "linux")]
fn detect_bus(sysfs_path: &Path) -> BusType {
    // Resolve the device ancestry once. For a USB mass-storage device the
    // canonical path normally contains `usb*` even though its immediate
    // storage subsystem may be SCSI. USB is therefore checked before SCSI.
    let device_path = sysfs_path.join("device");
    if let Ok(real) = fs::canonicalize(&device_path) {
        let components: Vec<String> = real
            .components()
            .filter_map(|component| component.as_os_str().to_str().map(str::to_owned))
            .collect();

        if components.iter().any(|c| c == "usb" || c.starts_with("usb")) {
            return BusType::Usb;
        }
        if components.iter().any(|c| c == "nvme" || c.starts_with("nvme")) {
            return BusType::Nvme;
        }
        if components.iter().any(|c| c == "ata" || c.starts_with("ata")) {
            return BusType::Sata;
        }
        if components.iter().any(|c| c == "mmio" || c.starts_with("mmio")) {
            return BusType::Mmio;
        }
        if components.iter().any(|c| c == "scsi" || c.starts_with("scsi")) {
            return BusType::Scsi;
        }
    }

    if sysfs_path
        .file_name()
        .and_then(|x| x.to_str())
        .is_some_and(|name| name.starts_with("nvme"))
    {
        BusType::Nvme
    } else {
        BusType::Unknown
    }
}

/// Scan `/sys/block` and identify all visible physical storage devices.
///
/// Virtual block devices such as loop devices, device-mapper devices, mdraid,
/// and zram are excluded. Results are sorted by kernel device name.
#[cfg(target_os = "linux")]
#[allow(dead_code)]
pub fn identify_all() -> Result<Vec<StorageDevice>, IdentifyError> {
    let mut names = fs::read_dir("/sys/block")
        .map_err(|source| IdentifyError::Io {
            path: PathBuf::from("/sys/block"),
            source,
        })?
        .filter_map(Result::ok)
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| {
            !(name.starts_with("loop")
                || name.starts_with("ram")
                || name.starts_with("zram")
                || name.starts_with("dm-")
                || name.starts_with("md"))
        })
        .collect::<Vec<_>>();

    names.sort_unstable();

    let mut devices = Vec::with_capacity(names.len());
    for name in names {
        let path = PathBuf::from("/dev").join(name);
        match identify(&path) {
            Ok(device) => devices.push(device),
            Err(IdentifyError::Io { .. }) | Err(IdentifyError::NotBlockDevice(_)) => {}
            Err(error) => return Err(error),
        }
    }

    Ok(devices)
}

/// Non-Linux stub for [`identify_all`].
#[cfg(not(target_os = "linux"))]
pub fn identify_all() -> Result<Vec<StorageDevice>, IdentifyError> {
    Err(IdentifyError::NotLinux)
}

fn classify(name: &str, bus: BusType, rotational: bool, removable: bool) -> StorageDeviceType {
    if bus == BusType::Nvme || name.starts_with("nvme") {
        return StorageDeviceType::Nvme;
    }

    if bus == BusType::Usb {
        if rotational {
            StorageDeviceType::ExternalHdd
        } else if removable {
            StorageDeviceType::UsbFlashDrive
        } else {
            StorageDeviceType::ExternalSsd
        }
    } else if rotational {
        StorageDeviceType::Hdd
    } else {
        StorageDeviceType::Ssd
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_nvme() {
        assert_eq!(classify("nvme0n1", BusType::Nvme, false, false), StorageDeviceType::Nvme);
    }

    #[test]
    fn classifies_usb_flash() {
        assert_eq!(classify("sdb", BusType::Usb, false, true), StorageDeviceType::UsbFlashDrive);
    }

    #[test]
    fn classifies_external_hdd() {
        assert_eq!(classify("sdb", BusType::Usb, true, false), StorageDeviceType::ExternalHdd);
    }

    #[test]
    fn classifies_internal_hdd() {
        assert_eq!(classify("sda", BusType::Sata, true, false), StorageDeviceType::Hdd);
    }

    #[test]
    fn classifies_internal_ssd() {
        assert_eq!(classify("sda", BusType::Sata, false, false), StorageDeviceType::Ssd);
    }
}