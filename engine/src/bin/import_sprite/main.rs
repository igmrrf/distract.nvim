mod decode;

use std::path::PathBuf;
use std::process;

const DEFAULT_BG_TOLERANCE: f32 = 0.12;

struct Args {
    gif: Option<PathBuf>,
    frames_dir: Option<PathBuf>,
    name: String,
    states: Option<String>,
    out: PathBuf,
    manifest_out: PathBuf,
    bg_tolerance: f32,
}

fn parse_args_from(mut raw_args: impl Iterator<Item = String>) -> Result<Args, String> {
    let mut gif = None;
    let mut frames_dir = None;
    let mut name: Option<String> = None;
    let mut states = None;
    let mut out = None;
    let mut manifest_out = None;
    let mut bg_tolerance = DEFAULT_BG_TOLERANCE;

    while let Some(flag) = raw_args.next() {
        let mut take_value = || {
            raw_args
                .next()
                .ok_or_else(|| format!("{flag} needs a value"))
        };
        match flag.as_str() {
            "--gif" => gif = Some(PathBuf::from(take_value()?)),
            "--frames" => frames_dir = Some(PathBuf::from(take_value()?)),
            "--name" => name = Some(take_value()?),
            "--states" => states = Some(take_value()?),
            "--out" => out = Some(PathBuf::from(take_value()?)),
            "--manifest-out" => manifest_out = Some(PathBuf::from(take_value()?)),
            "--bg-tolerance" => {
                bg_tolerance = take_value()?
                    .parse()
                    .map_err(|_| "--bg-tolerance needs a number".to_string())?;
            }
            other => return Err(format!("unknown flag '{other}'")),
        }
    }

    let name = name.ok_or("--name is required")?;
    if gif.is_some() == frames_dir.is_some() {
        return Err("exactly one of --gif or --frames is required".to_string());
    }

    let out = out.unwrap_or_else(|| PathBuf::from(format!("assets/{name}")));
    let manifest_out =
        manifest_out.unwrap_or_else(|| PathBuf::from(format!("lua/distract/manifests/{name}.lua")));

    Ok(Args {
        gif,
        frames_dir,
        name,
        states,
        out,
        manifest_out,
        bg_tolerance,
    })
}

fn parse_args() -> Result<Args, String> {
    parse_args_from(std::env::args().skip(1))
}

fn main() {
    let args = match parse_args() {
        Ok(args) => args,
        Err(message) => {
            eprintln!("import_sprite: {message}");
            process::exit(1);
        }
    };
    eprintln!("import_sprite: parsed args for asset '{}'", args.name);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(flags: &[&str]) -> Vec<String> {
        flags.iter().map(|flag| flag.to_string()).collect()
    }

    #[test]
    fn requires_exactly_one_of_gif_or_frames() {
        let neither = parse_args_from(args(&["--name", "x"]).into_iter());
        assert!(neither.is_err());

        let both = parse_args_from(
            args(&["--gif", "a.gif", "--frames", "dir", "--name", "x"]).into_iter(),
        );
        assert!(both.is_err());
    }

    #[test]
    fn defaults_out_and_manifest_out_from_name() {
        let parsed =
            parse_args_from(args(&["--gif", "a.gif", "--name", "cat_walking"]).into_iter())
                .expect("parse");
        assert_eq!(parsed.out, PathBuf::from("assets/cat_walking"));
        assert_eq!(
            parsed.manifest_out,
            PathBuf::from("lua/distract/manifests/cat_walking.lua")
        );
        assert_eq!(parsed.bg_tolerance, DEFAULT_BG_TOLERANCE);
    }

    #[test]
    fn explicit_flags_override_defaults() {
        let parsed = parse_args_from(
            args(&[
                "--frames",
                "dir",
                "--name",
                "x",
                "--out",
                "/tmp/out",
                "--manifest-out",
                "/tmp/x.lua",
                "--bg-tolerance",
                "0.2",
            ])
            .into_iter(),
        )
        .expect("parse");
        assert_eq!(parsed.out, PathBuf::from("/tmp/out"));
        assert_eq!(parsed.manifest_out, PathBuf::from("/tmp/x.lua"));
        assert_eq!(parsed.bg_tolerance, 0.2);
    }
}
