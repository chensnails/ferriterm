//! Risk grading.
//!
//! A command is graded from its program name plus the things that make the
//! same program dangerous: recursion, elevated privilege, a system path as the
//! target, a write landing on a device or a credential file. The result is a
//! level from [`Risk::R0`] (read only) to [`Risk::R4`] (destroy the machine),
//! plus the individual reasons so the approval card can explain itself.

use serde::{Deserialize, Serialize};

use crate::parse::{Command, Segment};

/// Read only. Safe to run without asking.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Risk {
    R0,
    R1,
    R2,
    R3,
    R4,
}

impl Risk {
    pub fn id(self) -> &'static str {
        match self {
            Risk::R0 => "r0",
            Risk::R1 => "r1",
            Risk::R2 => "r2",
            Risk::R3 => "r3",
            Risk::R4 => "r4",
        }
    }

    pub fn at_least(self, floor: Risk) -> Risk {
        if self > floor {
            self
        } else {
            floor
        }
    }

    pub fn highest(self, other: Risk) -> Risk {
        if other > self {
            other
        } else {
            self
        }
    }

    fn bump(self, levels: u8) -> Risk {
        let next = (self as u8).saturating_add(levels).min(Risk::R4 as u8);
        match next {
            0 => Risk::R0,
            1 => Risk::R1,
            2 => Risk::R2,
            3 => Risk::R3,
            _ => Risk::R4,
        }
    }
}

/// Why a command was graded the way it was. Serialized as a camelCase id that
/// the interface turns into a sentence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Reason {
    /// Deletes the filesystem root, a system directory or the home directory.
    RootDelete,
    /// Writes into `/etc`, `/usr`, `/boot`, … or to a raw device.
    SystemPath,
    /// Runs through `sudo`/`doas`/`su`, or the session is already root.
    Elevated,
    /// Recursive delete or recursive permission change.
    Recursive,
    /// Downloads and pipes straight into a shell.
    RemotePipe,
    /// Reads or writes a key, token or password file.
    SecretFile,
    /// Writes to a block device.
    DiskDevice,
    /// Fork bomb or an equivalent self-replicating one-liner.
    ForkBomb,
    /// Changes ownership, permissions or accounts.
    PrivilegeChange,
    /// Stops, disables or masks a system service.
    ServiceControl,
    /// Installs or removes software packages.
    PackageInstall,
    /// Deletes more than it should be able to undo.
    BulkDelete,
    /// A redirection that overwrites a system file.
    WriteRedirect,
    /// The program is not in any list, so it is treated as cautious.
    Unknown,
}

/// What the grader concluded about one command line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Assessment {
    pub risk: Risk,
    /// Irreversible enough that no rule may downgrade it.
    pub hard: bool,
    pub reasons: Vec<Reason>,
}

impl Default for Assessment {
    fn default() -> Self {
        Self {
            risk: Risk::R0,
            hard: false,
            reasons: Vec::new(),
        }
    }
}

impl Assessment {
    fn note(&mut self, reason: Reason) {
        if !self.reasons.contains(&reason) {
            self.reasons.push(reason);
        }
    }

    fn merge(&mut self, other: Assessment) {
        self.risk = self.risk.highest(other.risk);
        self.hard |= other.hard;
        for reason in other.reasons {
            self.note(reason);
        }
    }
}

const READ_ONLY: &[&str] = &[
    "ls", "ll", "la", "cat", "head", "tail", "less", "more", "grep", "egrep", "fgrep", "rg", "ag",
    "ps", "top", "htop", "df", "du", "free", "uptime", "whoami", "id", "groups", "uname", "hostname",
    "date", "cal", "echo", "printf", "pwd", "env", "printenv", "which", "type", "wc", "sort", "uniq",
    "cut", "tr", "jq", "file", "stat", "diff", "cmp", "man", "help", "history", "tree", "lsof",
    "netstat", "ss", "dig", "nslookup", "host", "nproc", "ulimit", "basename", "dirname", "realpath",
    "readlink", "sha256sum", "md5sum", "cksum", "find",
];

const WRITE_LOW: &[&str] = &[
    "mkdir", "touch", "ln", "tee", "zip", "gzip", "gunzip", "ssh-keygen", "base64", "curl",
    "wget", "scp", "sftp", "translate",
];

const WRITE_MID: &[&str] = &[
    "cp", "mv", "rm", "rmdir", "rsync", "tar", "unzip", "sed", "truncate", "kill", "pkill",
    "killall", "npm", "yarn", "pnpm", "pip", "pip3", "cargo", "go", "gem", "apt", "apt-get", "yum",
    "dnf", "apk", "brew", "pacman", "zypper", "docker", "podman", "kubectl", "helm", "terraform",
    "make", "cmake", "python", "python3", "node", "bash", "sh", "zsh", "fish", "systemctl",
    "service", "git", "mount", "umount", "dd",
];

const WRITE_HIGH: &[&str] = &[
    "chown", "useradd", "userdel", "usermod", "groupadd", "passwd", "visudo", "crontab", "at",
    "iptables", "ip6tables", "nft", "ufw", "firewall-cmd", "shutdown", "reboot", "poweroff", "halt",
    "init", "telinit", "swapoff", "fdisk", "parted", "systemd-run", "nsenter",
];

const FATAL_PROGRAMS: &[&str] = &["mkfs", "wipefs", "shred", "mke2fs"];

/// Commands that only report, unless a flag turns them destructive.
const READ_ONLY_WITH_FLAGS: &[(&str, &[&str])] = &[
    ("find", &["-delete", "-exec", "-execdir", "-ok"]),
    ("tar", &["-c", "--create"]),
    ("git", &["commit", "push", "reset", "clean", "checkout", "rebase", "merge"]),
    ("docker", &["run", "rm", "rmi", "prune", "system", "start", "stop", "kill"]),
    ("kubectl", &["apply", "delete", "edit", "patch", "scale"]),
    ("systemctl", &["start", "stop", "restart", "reload", "disable", "mask", "isolate"]),
];

const SYSTEM_ROOTS: &[&str] = &[
    "/etc", "/boot", "/dev", "/sys", "/proc", "/usr", "/bin", "/sbin", "/lib", "/lib64", "/opt",
    "/srv", "/var/lib", "/var/spool", "/root",
];

pub fn is_system_path(word: &str) -> bool {
    let trimmed = word.trim_end_matches('/');
    if trimmed.is_empty() {
        // "/" or "///" — the root itself.
        return word.starts_with('/');
    }
    SYSTEM_ROOTS
        .iter()
        .any(|root| trimmed == *root || trimmed.starts_with(&format!("{root}/")))
}

/// The directory itself, not one of its files: moving or deleting `/etc` is
/// not the same class of mistake as deleting `/etc/motd`.
fn is_system_root(word: &str) -> bool {
    let trimmed = word.trim_end_matches('/');
    SYSTEM_ROOTS.iter().any(|root| trimmed == *root)
}

fn is_device(word: &str) -> bool {
    word.starts_with("/dev/") && word != "/dev/null" && word != "/dev/stdout" && word != "/dev/stderr"
}

fn is_root_target(word: &str) -> bool {
    let trimmed = word.trim();
    trimmed == "/"
        || trimmed == "/*"
        || trimmed == "/."
        || trimmed == "/.."
        || trimmed == "//"
        || trimmed.starts_with("/*")
}

fn is_home_root(word: &str) -> bool {
    let trimmed = word.trim_end_matches('/');
    trimmed == "~" || trimmed == "$HOME" || trimmed == "${HOME}"
}

fn is_secret(word: &str) -> bool {
    let lower = word.to_ascii_lowercase();
    lower.contains("id_rsa")
        || lower.contains("id_ed25519")
        || lower.contains(".aws/credentials")
        || lower.contains(".ssh/")
        || lower.ends_with(".pem")
        || lower.ends_with(".key")
        || lower.contains("/etc/shadow")
        || lower.contains("/etc/sudoers")
        || lower.contains(".pgpass")
        || lower.contains(".netrc")
}

fn has_recursive_flag(segment: &Segment) -> bool {
    segment.has_any_flag(&["-r", "-R", "--recursive"])
        || segment
            .rest()
            .iter()
            .any(|word| word.starts_with('-') && word.contains('r') && word.len() <= 5 && word.starts_with("-r"))
}

fn base_of(segment: &Segment) -> Assessment {
    let program = segment.program().to_ascii_lowercase();
    let mut assessment = Assessment {
        risk: Risk::R2,
        hard: false,
        reasons: vec![Reason::Unknown],
    };

    if program.is_empty() {
        return Assessment::default();
    }
    if FATAL_PROGRAMS.iter().any(|fatal| program.starts_with(fatal)) {
        assessment.risk = Risk::R4;
        assessment.hard = true;
        assessment.reasons = vec![Reason::DiskDevice];
        return assessment;
    }
    if READ_ONLY.iter().any(|name| *name == program) {
        assessment.risk = Risk::R0;
        assessment.reasons.clear();
    } else if WRITE_LOW.iter().any(|name| *name == program) {
        assessment.risk = Risk::R1;
        assessment.reasons.clear();
    } else if WRITE_MID.iter().any(|name| *name == program) {
        assessment.risk = Risk::R2;
        assessment.reasons.clear();
    } else if WRITE_HIGH.iter().any(|name| *name == program) {
        assessment.risk = Risk::R3;
        assessment.reasons.clear();
    }
    if program == "chmod" || program == "chown" {
        assessment.reasons.clear();
        assessment.risk = Risk::R2;
    }
    if program == "chown" || program == "useradd" || program == "passwd" {
        assessment.note(Reason::PrivilegeChange);
    }
    if program == "systemctl" || program == "service" {
        assessment.note(Reason::ServiceControl);
    }
    if matches!(
        program.as_str(),
        "apt" | "apt-get" | "yum" | "dnf" | "apk" | "brew" | "pacman" | "zypper" | "pip" | "pip3"
            | "npm" | "yarn" | "pnpm" | "gem"
    ) {
        assessment.note(Reason::PackageInstall);
    }
    // A read-only program with a destructive flag is not read only.
    for (name, flags) in READ_ONLY_WITH_FLAGS {
        if program == *name {
            let destructive = segment
                .rest()
                .iter()
                .any(|word| flags.iter().any(|flag| word == flag));
            if destructive {
                assessment.risk = Risk::R2.at_least(assessment.risk).at_least(Risk::R2);
                if *name == "find" {
                    assessment.risk = Risk::R2;
                }
                assessment.note(Reason::BulkDelete);
            }
            break;
        }
    }
    assessment
}

/// Grade one segment, including the commands nested inside it.
fn grade_segment(segment: &Segment) -> Assessment {
    let program = segment.program().to_ascii_lowercase();

    // A wrapper is transparent for grading: `sudo rm -rf /` is `rm -rf /`
    // with everything one level higher.
    if matches!(program.as_str(), "sudo" | "doas" | "pkexec" | "su") {
        let mut assessment = Assessment::default();
        assessment.note(Reason::Elevated);
        let mut rest: Vec<String> = segment.rest().to_vec();
        // Drop su/sudo option words so the wrapped program is first.
        while rest.first().map(|word| word.starts_with('-')).unwrap_or(false) {
            rest.remove(0);
        }
        if let Some(command) = rest.first() {
            let mut inner = Segment {
                argv: rest.clone(),
                raw: segment.raw.clone(),
                redirects: segment.redirects.clone(),
                nested: segment.nested.clone(),
            };
            if command == "-c" {
                inner.argv.remove(0);
            }
            let mut nested = grade_segment(&inner);
            nested.risk = nested.risk.bump(1);
            assessment.merge(nested);
        }
        return assessment;
    }

    // `sh -c "…"` carries its payload as a string; grade what it really runs.
    let shell_like = matches!(program.as_str(), "sh" | "bash" | "zsh" | "dash" | "fish");
    if shell_like {
        if let Some(position) = segment.rest().iter().position(|word| word == "-c") {
            if let Some(script) = segment.rest().get(position + 1) {
                let parsed = crate::parse::parse(script);
                return grade_command(&parsed);
            }
        }
    }

    let mut assessment = base_of(segment);

    if has_recursive_flag(segment) {
        assessment.note(Reason::Recursive);
        if matches!(program.as_str(), "rm" | "rmdir") {
            assessment.risk = assessment.risk.at_least(Risk::R3);
            assessment.note(Reason::BulkDelete);
        } else if matches!(program.as_str(), "chmod" | "chown") {
            assessment.risk = assessment.risk.at_least(Risk::R3);
            assessment.note(Reason::PrivilegeChange);
        }
    }

    // Touching a system path only matters for programs that can change it.
    // `cat /etc/hosts` is a read; `cp x /etc/hosts` is not.
    let writes = assessment.risk >= Risk::R1;

    for word in &segment.argv {
        if is_secret(word) {
            assessment.note(Reason::SecretFile);
            assessment.risk = assessment.risk.bump(1);
        }
        if is_device(word) {
            assessment.note(Reason::DiskDevice);
            if writes {
                assessment.risk = Risk::R4;
            }
        }
        if is_system_path(word) && writes {
            assessment.note(Reason::SystemPath);
            assessment.risk = assessment.risk.bump(1).at_least(Risk::R2);
        }
        if (is_root_target(word) || is_home_root(word))
            && matches!(program.as_str(), "rm" | "rmdir" | "mv" | "chmod" | "chown")
        {
            assessment.note(Reason::RootDelete);
            assessment.risk = Risk::R4;
            assessment.hard = true;
        }
    }

    for redirect in &segment.redirects {
        if is_system_path(&redirect.target) {
            assessment.note(Reason::WriteRedirect);
            assessment.note(Reason::SystemPath);
            assessment.risk = assessment.risk.bump(1).at_least(Risk::R3);
        }
        if is_device(&redirect.target) {
            assessment.note(Reason::DiskDevice);
            assessment.risk = Risk::R4;
            assessment.hard = true;
        }
    }

    // `rm -rf /etc/nginx` deletes a system tree; `rm /etc/motd` overwrites one
    // file and is bad but not fatal.
    if matches!(program.as_str(), "rm" | "rmdir" | "mv")
        && segment.rest().iter().any(|word| is_system_root(word))
    {
        assessment.hard = true;
        assessment.risk = Risk::R4;
        assessment.note(Reason::SystemPath);
    }

    if matches!(program.as_str(), "rm" | "rmdir")
        && has_recursive_flag(segment)
        && segment
            .rest()
            .iter()
            .any(|word| is_system_path(word) || is_root_target(word))
    {
        assessment.hard = true;
        assessment.risk = Risk::R4;
        assessment.note(Reason::RootDelete);
    }

    if program == "dd" {
        if segment
            .rest()
            .iter()
            .any(|word| word.starts_with("of=/dev/") || word.starts_with("of= /dev/"))
        {
            assessment.hard = true;
            assessment.risk = Risk::R4;
            assessment.note(Reason::DiskDevice);
        } else {
            assessment.risk = Risk::R2.at_least(assessment.risk);
        }
    }

    for nested in &segment.nested {
        let mut inner = grade_segment(nested);
        // A command substitution inside a read-only command (echo, cat) still
        // executes.
        inner.risk = inner.risk.bump(0);
        assessment.merge(inner);
    }

    assessment
}

fn grade_command(command: &Command) -> Assessment {
    let mut assessment = Assessment::default();
    let mut saw_download = false;

    for segment in &command.segments {
        let program = segment.program().to_ascii_lowercase();
        if program == "curl" || program == "wget" {
            saw_download = true;
        } else if saw_download
            && matches!(program.as_str(), "sh" | "bash" | "zsh" | "dash" | "python" | "python3" | "perl" | "ruby")
        {
            assessment.note(Reason::RemotePipe);
            assessment.risk = Risk::R4;
            saw_download = false;
        }
        assessment.merge(grade_segment(segment));
    }

    if is_fork_bomb(&command.normalized()) {
        assessment.hard = true;
        assessment.risk = Risk::R4;
        assessment.note(Reason::ForkBomb);
    }

    if assessment.reasons.is_empty() {
        assessment.note(Reason::Unknown);
    }
    assessment
}

fn is_fork_bomb(text: &str) -> bool {
    let compact: String = text.chars().filter(|ch| !ch.is_whitespace()).collect();
    compact.contains(":(){:|:&};:") || compact.contains("(){(:&)}") || compact.contains(":|:&")
}

pub fn assess(command: &Command) -> Assessment {
    grade_command(command)
}

pub fn assess_line(line: &str) -> Assessment {
    // Fork bombs survive the splitter as debris, so the raw text is checked
    // before parsing rather than after.
    if is_fork_bomb(line) {
        return Assessment {
            risk: Risk::R4,
            hard: true,
            reasons: vec![Reason::ForkBomb],
        };
    }
    assess(&crate::parse::parse(line))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grade(line: &str) -> Assessment {
        assess_line(line)
    }

    #[test]
    fn reads_are_free() {
        for line in ["ls -la", "cat /etc/hosts", "ps aux | grep node", "df -h"] {
            let assessment = grade(line);
            assert_eq!(assessment.risk, Risk::R0, "{line}");
            assert!(!assessment.hard, "{line}");
        }
    }

    #[test]
    fn creating_is_low_but_overwriting_asks() {
        assert_eq!(grade("mkdir -p build").risk, Risk::R1);
        assert_eq!(grade("touch notes.md").risk, Risk::R1);
        assert_eq!(grade("cp a.txt b.txt").risk, Risk::R2);
        assert_eq!(grade("tar -xzf app.tgz").risk, Risk::R2);
    }

    #[test]
    fn deletes_and_installs_ask() {
        assert_eq!(grade("rm build.log").risk, Risk::R2);
        assert_eq!(grade("mv a b").risk, Risk::R2);
        assert_eq!(grade("apt-get install nginx").risk, Risk::R2);
        assert_eq!(grade("systemctl restart nginx").risk, Risk::R2);
    }

    #[test]
    fn recursion_raises_a_delete() {
        let assessment = grade("rm -rf build/");
        assert_eq!(assessment.risk, Risk::R3);
        assert!(assessment.reasons.contains(&Reason::Recursive));
        assert!(!assessment.hard, "a project directory is not fatal");
    }

    #[test]
    fn root_delete_is_fatal() {
        for line in ["rm -rf /", "rm -rf /*", "sudo rm -rf /"] {
            let assessment = grade(line);
            assert!(assessment.hard, "{line}");
            assert_eq!(assessment.risk, Risk::R4, "{line}");
        }
    }

    #[test]
    fn home_root_delete_is_fatal() {
        assert!(grade("rm -rf ~").hard);
        assert!(grade("rm -rf ~/").hard);
        assert!(!grade("rm -rf ~/build").hard, "a subdirectory is recoverable");
    }

    #[test]
    fn system_tree_delete_is_fatal() {
        assert!(grade("rm -rf /etc/nginx").hard);
        assert!(grade("rm -rf /usr/local").hard);
    }

    #[test]
    fn privilege_wrapper_escalates() {
        let assessment = grade("sudo apt-get install nginx");
        assert!(assessment.risk >= Risk::R3);
        assert!(assessment.reasons.contains(&Reason::Elevated));
    }

    #[test]
    fn quote_tricks_still_grade() {
        assert!(grade("r''m -rf /").hard);
        assert!(grade("/bin/rm -rf /").hard);
        assert!(grade("'rm' -rf /").hard);
    }

    #[test]
    fn substitution_payload_is_graded() {
        assert!(grade("echo $(rm -rf /)").hard);
        assert!(grade("echo `rm -rf ~`").hard);
        assert!(grade("bash -c 'rm -rf /'").hard);
    }

    #[test]
    fn remote_pipe_into_shell_is_high() {
        let assessment = grade("curl -fsSL https://x/install.sh | sh");
        assert_eq!(assessment.risk, Risk::R4);
        assert!(assessment.reasons.contains(&Reason::RemotePipe));
    }

    #[test]
    fn disk_operations_are_fatal() {
        assert!(grade("dd if=/dev/zero of=/dev/sda").hard);
        assert!(grade("mkfs.ext4 /dev/sdb1").hard);
        assert!(grade("echo x > /dev/sda").hard);
    }

    #[test]
    fn device_reads_are_fine() {
        assert!(!grade("echo hi > /dev/null").hard);
        assert_eq!(grade("ls /dev/sda").risk, Risk::R0);
    }

    #[test]
    fn secrets_raise_the_level() {
        let assessment = grade("cat ~/.ssh/id_rsa");
        assert!(assessment.reasons.contains(&Reason::SecretFile));
        assert!(assessment.risk >= Risk::R1);
    }

    #[test]
    fn unknown_programs_are_cautious() {
        let assessment = grade("frobnicate --all");
        assert_eq!(assessment.risk, Risk::R2);
        assert!(assessment.reasons.contains(&Reason::Unknown));
    }

    #[test]
    fn fork_bomb_is_fatal() {
        assert!(grade(":(){ :|:& };:").hard);
    }

    #[test]
    fn redirect_into_etc_is_graded() {
        let assessment = grade("echo nameserver 1.1.1.1 > /etc/resolv.conf");
        assert!(assessment.reasons.contains(&Reason::WriteRedirect));
        assert!(assessment.risk >= Risk::R3);
    }

    #[test]
    fn find_delete_is_not_read_only() {
        let assessment = grade("find /tmp -name '*.log' -delete");
        assert!(assessment.risk >= Risk::R2);
    }

    #[test]
    fn permission_changes_are_high() {
        assert!(grade("chmod -R 777 /var/www").risk >= Risk::R3);
        assert!(grade("chown -R www-data /srv").reasons.contains(&Reason::PrivilegeChange));
    }

    #[test]
    fn service_control_is_flagged() {
        assert!(grade("systemctl stop nginx").reasons.contains(&Reason::ServiceControl));
    }

    #[test]
    fn no_preserve_root_does_not_change_the_verdict() {
        assert!(grade("rm -rf / --no-preserve-root").hard);
    }

    #[test]
    fn moving_a_system_tree_is_fatal() {
        assert!(grade("mv /etc /tmp/etc.bak").hard);
    }

    #[test]
    fn find_over_the_root_is_graded() {
        let assessment = grade("find / -name '*.log' -delete");
        assert!(assessment.risk >= Risk::R2);
        assert!(assessment.reasons.contains(&Reason::BulkDelete));
    }

    /// Known limit: an indirection through a variable cannot be resolved
    /// without a shell. It degrades to "unknown program, cautious", which is
    /// why the approval card always shows the literal command.
    #[test]
    fn variable_indirection_degrades_to_unknown() {
        let assessment = grade("R=rm; $R -rf /");
        assert!(assessment.reasons.contains(&Reason::Unknown));
        assert!(assessment.risk >= Risk::R2);
    }
}
