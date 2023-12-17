use std::path::PathBuf;

use super::util::try_interaction;
use nu_engine::env::current_dir;
use nu_engine::CallExt;
use nu_protocol::ast::Call;
use nu_protocol::engine::{Command, EngineState, Stack};
use nu_protocol::{
    Category, Example, PipelineData, ShellError, Signature, Spanned, SyntaxShape, Type,
};

#[derive(Clone)]
pub struct Ln;

impl Command for Ln {
    fn name(&self) -> &str {
        "ln"
    }

    fn usage(&self) -> &str {
        "Create a link to a file or directory."
    }

    fn search_terms(&self) -> Vec<&str> {
        vec!["link", "symlink"]
    }

    fn signature(&self) -> nu_protocol::Signature {
        Signature::build("ln")
            .input_output_types(vec![(Type::Nothing, Type::Nothing)])
            .required(
                "target",
                // TODO: Take a FilePath instead of a string here.
                // For now we're using a string because nu resolves relative paths for us which we don't want.
                SyntaxShape::String,
                "The file or directory to link to.",
            )
            .required(
                "link_name",
                SyntaxShape::String,
                "The name of the link to create.",
            )
            .switch(
                "verbose",
                "make mv to be verbose, showing files been moved.",
                Some('v'),
            )
            .switch(
                "directory",
                "make hard links to directories instead of files.",
                Some('d'),
            )
            .switch("force", "remove existing destination files", Some('f'))
            .switch("interactive", "ask user to confirm action", Some('i'))
            .switch(
                "relative",
                "create symbolic links relative to link location",
                Some('r'),
            )
            .switch(
                "symbolic",
                "make symbolic links instead of hard links",
                Some('s'),
            )
            .named(
                "target-directory",
                SyntaxShape::Filepath,
                "move all source arguments into directory",
                Some('t'),
            )
            .switch(
                "no-target-directory",
                "treat link name as a normal file if it is a symbolic link to a directory",
                Some('T'),
            )
            .category(Category::FileSystem)
    }

    fn run(
        &self,
        engine_state: &EngineState,
        stack: &mut Stack,
        call: &Call,
        _input: PipelineData,
    ) -> Result<PipelineData, ShellError> {
        let Some(LnParsedArgs {
            verbose,
            directory,
            symbolic,
            linkname,
            target,
        }) = setup_paths(call, engine_state, stack, try_interaction)?
        else {
            return Ok(PipelineData::empty());
        };

        if symbolic {
            if directory {
                #[cfg(unix)]
                std::os::unix::fs::symlink(&target, &linkname)?;
                #[cfg(windows)]
                std::os::windows::fs::symlink_dir(target, linkname)?;
            } else {
                #[cfg(unix)]
                std::os::unix::fs::symlink(&target, &linkname)?;
                #[cfg(windows)]
                std::os::windows::fs::symlink_file(target, linkname)?;
            }
        } else {
            std::fs::hard_link(&target, &linkname)?;
        }

        if verbose {
            println!(
                "'{}' -> '{}'{}",
                linkname.to_string_lossy(),
                target.to_string_lossy(),
                if symbolic { " (symbolic link)" } else { "" }
            );
        }

        Ok(PipelineData::empty())
    }

    fn examples(&self) -> Vec<Example> {
        vec![
            Example {
                description: "Rename a file",
                example: "mv before.txt after.txt",
                result: None,
            },
            Example {
                description: "Move a file into a directory",
                example: "mv test.txt my/subdirectory",
                result: None,
            },
            Example {
                description: "Move many files into a directory",
                example: "mv *.txt my/subdirectory",
                result: None,
            },
        ]
    }
}

#[derive(Debug, PartialEq, Eq)]
struct LnParsedArgs {
    verbose: bool,
    directory: bool,
    symbolic: bool,
    linkname: std::path::PathBuf,
    target: std::path::PathBuf,
}

fn setup_paths(
    call: &Call,
    engine_state: &EngineState,
    stack: &mut Stack,
    // pub fn try_interaction(interactive: bool, prompt: String) -> (Result<Option<bool>, Box<dyn Error>>, bool)
    interact: impl Fn(bool, String) -> (Result<Option<bool>, Box<dyn std::error::Error>>, bool),
) -> Result<Option<LnParsedArgs>, ShellError> {
    let spanned_target: Spanned<String> = call.req(engine_state, stack, 0)?;
    let spanned_linkname: Spanned<String> = call.req(engine_state, stack, 1)?;
    let verbose = call.has_flag("verbose");
    let interactive = call.has_flag("interactive");
    let force = call.has_flag("force");
    let directory = call.has_flag("directory");
    let symbolic = call.has_flag("symbolic");
    let relative = call.has_flag("relative");
    let target_directory: Option<String> =
        call.get_flag(engine_state, stack, "target-directory")?;
    let no_target_directory = call.has_flag("no-target-directory");
    let cwd = current_dir(engine_state, stack)?;
    let mut linkname = cwd.join(spanned_linkname.item.as_str());
    println!("target : {}", spanned_target.item.as_str());
    let target = PathBuf::from(spanned_target.item.as_str());
    let target = if target.is_relative() {
        if relative {
            target
        } else {
            cwd.join(target)
        }
    } else {
        target
    };
    println!("target 2 : {}", target.display());
    if !no_target_directory && linkname.is_dir() {
        linkname.push(target.file_name().unwrap());
    };
    if !target.exists() {
        return Err(ShellError::FileNotFoundCustom {
            span: spanned_target.span,
            msg: format!("No such file or directory: {}", target.display()),
        });
    }
    if linkname.exists() {
        if interactive {
            let (interaction, confirmed) = interact(
                interactive,
                format!("ln: overwrite '{}'", linkname.to_string_lossy()),
            );
            if let Err(e) = interaction {
                return Err(ShellError::GenericError {
                    error: format!("Error during interaction: {e:}"),
                    msg: "failed to get confirmation".to_string(),
                    span: None,
                    help: None,
                    inner: vec![],
                });
            } else if !confirmed {
                return Ok(None);
            }
        } else if !force {
            return Err(ShellError::FileAlreadyExists {
                span: spanned_linkname.span,
            });
        }

        // rm the existing file
        std::fs::remove_file(&linkname)?;
    }
    if let Some(target_directory) = target_directory {
        let target_directory = cwd.join(target_directory);
        if !target_directory.exists() {
            return Err(ShellError::DirectoryNotFound {
                dir: target_directory.to_string_lossy().to_string(),
                span: spanned_target.span,
            });
        }
        linkname = target_directory.join(linkname.file_name().unwrap());
    }

    Ok(Some(LnParsedArgs {
        verbose,
        directory,
        symbolic,
        linkname,
        target,
    }))
}
