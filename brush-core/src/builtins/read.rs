use clap::Parser;
use std::collections::VecDeque;
use std::io::Read;

use crate::{builtin, commands, env, error, openfiles, variables};

/// Parse standard input.
#[derive(Parser)]
pub(crate) struct ReadCommand {
    #[clap(short = 'a')]
    array_variable: Option<String>,

    #[clap(short = 'd')]
    delimiter: Option<String>,

    #[clap(short = 'e')]
    use_readline: bool,

    #[clap(short = 'i')]
    initial_text: Option<String>,

    #[clap(short = 'n')]
    return_after_n_chars: Option<usize>,

    #[clap(short = 'N')]
    return_after_n_chars_no_delimiter: Option<usize>,

    #[clap(short = 'p')]
    prompt: Option<String>,

    #[clap(short = 'r')]
    raw_mode: bool,

    #[clap(short = 's')]
    silent: bool,

    #[clap(short = 't')]
    timeout_in_seconds: Option<usize>,

    #[clap(short = 'u')]
    fd_num_to_read: Option<u8>,

    variable_names: Vec<String>,
}

#[async_trait::async_trait]
impl builtin::Command for ReadCommand {
    async fn execute(
        &self,
        context: commands::ExecutionContext<'_>,
    ) -> Result<crate::builtin::ExitCode, crate::error::Error> {
        if self.array_variable.is_some() {
            return error::unimp("read -a");
        }
        if self.delimiter.is_some() {
            return error::unimp("read -d");
        }
        if self.use_readline {
            return error::unimp("read -e");
        }
        if self.initial_text.is_some() {
            return error::unimp("read -i");
        }
        if self.return_after_n_chars.is_some() {
            return error::unimp("read -n");
        }
        if self.return_after_n_chars_no_delimiter.is_some() {
            return error::unimp("read -N");
        }
        if self.prompt.is_some() {
            return error::unimp("read -p");
        }
        if self.raw_mode {
            tracing::debug!("read -r is not implemented");
        }
        if self.timeout_in_seconds.is_some() {
            return error::unimp("read -t");
        }
        if self.fd_num_to_read.is_some() {
            return error::unimp("read -u");
        }

        let input_line = self.read_line(context.stdin())?;
        if let Some(input_line) = input_line {
            let mut variable_names: VecDeque<String> = self.variable_names.clone().into();
            let mut spillover: Option<String> = None;
            for field in input_line.split_ascii_whitespace() {
                if let Some(variable_name) = variable_names.pop_front() {
                    context.shell.env.update_or_add(
                        variable_name,
                        variables::ShellValueLiteral::Scalar(field.to_owned()),
                        |_| Ok(()),
                        env::EnvironmentLookup::Anywhere,
                        env::EnvironmentScope::Global,
                    )?;
                } else {
                    match &mut spillover {
                        Some(s) => {
                            s.push(' ');
                            s.push_str(field);
                        }
                        None => spillover = Some(field.to_owned()),
                    }
                }
            }

            if let Some(spillover) = spillover {
                context.shell.env.update_or_add(
                    "REPLY",
                    variables::ShellValueLiteral::Scalar(spillover),
                    |_| Ok(()),
                    env::EnvironmentLookup::Anywhere,
                    env::EnvironmentScope::Global,
                )?;
            }

            Ok(crate::builtin::ExitCode::Success)
        } else {
            Ok(crate::builtin::ExitCode::Custom(1))
        }
    }
}

impl ReadCommand {
    fn read_line(&self, mut file: openfiles::OpenFile) -> Result<Option<String>, error::Error> {
        let orig_term_attr = file.get_term_attr()?;
        if let Some(orig_term_attr) = &orig_term_attr {
            let mut updated_term_attr = orig_term_attr.to_owned();

            updated_term_attr
                .local_flags
                .remove(nix::sys::termios::LocalFlags::ICANON);
            updated_term_attr
                .local_flags
                .remove(nix::sys::termios::LocalFlags::ISIG);

            if self.silent {
                updated_term_attr
                    .local_flags
                    .remove(nix::sys::termios::LocalFlags::ECHO);
            }

            file.set_term_attr(nix::sys::termios::SetArg::TCSANOW, &updated_term_attr)?;
        }

        let mut line = String::new();
        let mut buffer = [0; 1]; // 1-byte buffer

        // TODO: Look at ignoring errors here.
        while let Ok(n) = file.read(&mut buffer) {
            if n == 0 {
                break; // EOF reached.
            }

            let ch = buffer[0] as char;

            if ch == '\x03' {
                return Ok(None); // Ctrl+C aborts.
            }

            if ch == '\n' {
                break; // End of line reached
            }

            // Ignore other control characters.
            if ch.is_ascii_control() {
                continue;
            }

            line.push(ch);
        }

        if let Some(orig_term_attr) = &orig_term_attr {
            file.set_term_attr(nix::sys::termios::SetArg::TCSANOW, orig_term_attr)?;
        }

        if line.is_empty() {
            Ok(None)
        } else {
            Ok(Some(line))
        }
    }
}
