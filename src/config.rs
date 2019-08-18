use serde::{Serialize, Deserialize};
use std::path::PathBuf;

fn default_core() -> usize { 4 }
fn default_memory() -> usize { 1024 }
fn default_cmdline() -> String {
    "console=hvc0 rw root=/dev/vda".to_owned()
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Config {
    /// Number of cores.
    #[serde(default = "default_core")]
    pub core: usize,

    /// Location of kernel.
    /// It should be of ELF format, not containing any firmware.
    pub kernel: PathBuf,

    /// Memory size, in MiB.
    #[serde(default = "default_memory")]
    pub memory: usize,

    /// Linux boot command line
    #[serde(default = "default_cmdline")]
    pub cmdline: String,

    /// Block devices
    #[serde(default)]
    pub drive: Vec<DriveConfig>,

    /// Random devices
    #[serde(default)]
    pub random: Vec<RandomConfig>,

    /// 9p file sharing
    #[serde(default)]
    pub share: Vec<ShareConfig>,

    /// Network adapters
    #[serde(default)]
    pub network: Vec<NetworkConfig>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct DriveConfig {
    /// Whether changes should be written back to the file.
    #[serde(default)]
    pub shadow: bool,

    /// Path to backing file.
    pub path: PathBuf,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "lowercase")]
pub enum RandomType {
    Pseudo,
    OS,
}

fn default_seed() -> u64 {
    0xcafebabedeadbeef
}

#[derive(Serialize, Deserialize, Debug)]
pub struct RandomConfig {
    pub r#type: RandomType,
    #[serde(default = "default_seed")]
    pub seed: u64,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ShareConfig {
    /// 9p sharing tag
    pub tag: String,

    /// Path to the shared directory
    pub path: PathBuf,
}

fn default_mac() -> String {
    "02:00:00:00:00:01".to_owned()
}

#[derive(Serialize, Deserialize, Debug)]
pub struct NetworkConfig {
    /// MAC address. For convience, we first parse it as string.
    #[serde(default = "default_mac")]
    pub mac: String,
}