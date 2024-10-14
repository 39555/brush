use crate::{builtins, commands};
use clap::Parser;
use std::io::Write;

/// Display the current working directory.
#[derive(Parser)]
pub(crate) struct PwdCommand {
    /// Print $PWD if it names the current working directory.
    // keep symlinks
    #[arg(short = 'L', overrides_with = "physical")]
    keep_symlinks: bool,

    /// Print the physical directory without any symlinks.
    // resolve all symlinks
    #[arg(short = 'P', overrides_with = "keep_symlinks")]
    physical: bool,
}

// pwd -L will give your logical working directory (respecting symlinks).
// pwd -P will give your physical working directory (ignoring symlinks).
// To keep things confusing, the bash builtin pwd defaults to pwd -L while /bin/pwd on an ubuntu
// system defaults to pwd -P. https://unix.stackexchange.com/questions/780711/bash-cd-l-vs-cd-p-vs-bash-reference-manual-description

// https://www.reddit.com/r/linux/comments/12z8ws7/stupid_linux_tricks_cd_one_shell_to_the_current/
// https://bash.cyberciti.biz/guide/Cd_command
// https://superuser.com/questions/830183/how-do-you-cd-change-directory-into-the-absolute-path-of-a-symbolic-linked-direc
// https://askubuntu.com/questions/592226/what-does-cd-l-p-e-do
// https://unix.stackexchange.com/questions/356595/examples-of-options-to-bash-cd-eg-cd-pe-directory
// https://man7.org/linux/man-pages/man1/cd.1p.html
// https://www.reddit.com/r/linux/comments/99jcu/cd_takes_you_back_to_the_previous_directory_you/
// https://superuser.com/questions/1312196/linux-symbolic-links-how-to-go-to-the-pointed-to-directory
// https://unix.stackexchange.com/questions/737144/what-does-resolve-symlink-mean

// implementations
// https://github.com/wertarbyte/coreutils/blob/master/src/pwd.c
// https://github.com/lattera/freebsd/blob/master/bin/pwd/pwd.c
// https://codebrowser.dev/glibc/glibc/io/pwd.c.html
// https://opensource.apple.com/source/shell_cmds/shell_cmds-118/pwd/pwd.c.auto.html

// cd https://github.com/nushell/nushell/blob/bdbcf829673c0a51805499832c20fab8a010733d/crates/nu-command/src/filesystem/cd.rs#L43
// pwd https://github.com/fish-shell/fish-shell/blob/2dafe81f97e16e87984cb0b4c656cec65d1371ad/src/builtins/pwd.rs#L15
// cd https://github.com/fish-shell/fish-shell/blob/2dafe81f97e16e87984cb0b4c656cec65d1371ad/src/builtins/cd.rs#L16

impl builtins::Command for PwdCommand {
    async fn execute(
        &self,
        context: commands::ExecutionContext<'_>,
    ) -> Result<crate::builtins::ExitCode, crate::error::Error> {
        // dbg!(&context.shell.get_current_working_dir());
        // POSIX: https://pubs.opengroup.org/onlinepubs/9699919799/utilities/pwd.html

        // TODO: look for 'physical' option in execution context options (set -P)

        // By default it should be the logical directory.
        // The logical working directory is maintained by the shell

        // if POSIXLY_CORRECT is set, we want to a logical resolution.
        // This produces a different output when doing mkdir -p a/b && ln -s a/b c && cd c && pwd
        // We should get c in this case instead of a/b at the end of the path
        let cwd = if self.physical && !context.shell.env.get_str("POSIXLY_CORRECT").is_some() {
            context.shell.get_current_working_dir()
        // -L logical by default or when POSIXLY_CORRECT is set
        } else {
            context.shell.get_current_logical_working_dir()
        };

        // \\?\ is a prefix Windows gives to paths under certain circumstances,
        // including when canonicalizing them.
        // With the right extension trait we can remove it non-lossily, but
        // we print it lossily anyway, so no reason to bother.
        // #[cfg(windows)]
        // let cwd = cwd
        //     .to_string_lossy()
        //     .strip_prefix(r"\\?\")
        //     .map(Into::into)
        //     .unwrap_or(cwd);

        writeln!(context.stdout(), "{}", cwd.display())?;

        Ok(builtins::ExitCode::Success)
    }
}
