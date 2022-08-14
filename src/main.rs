use std::io::Write;

use clap::{Parser, Subcommand};
use rusted_dectris::common::DumpRecordFile;

#[derive(Parser)]
#[clap(author, version, about, long_about = None)]
struct Cli {
    #[clap(subcommand)]
    action: Action,
    filename: String,
}

#[derive(Subcommand)]
enum Action {
    Cat {
        /// start at this message index (zero-based, inclusive)
        start_idx: usize,

        /// stop at this message index (zero-based, inclusive)
        end_idx: usize,
    },
    Inspect {
        /// display the first N messages
        #[clap(short, long)]
        head: Option<usize>,

        /// display a summary of all messages
        #[clap(short, long, action)]
        summary: bool,
    },
    Repeat {
        repetitions: usize,
    },
    Sim {
        uri: String,
    },
}

pub fn action_cat(
    cli: &Cli,
    start_idx: usize,
    end_idx: usize
) {
    let file = DumpRecordFile::new(&cli.filename);
    let mut cursor = file.get_cursor();

    eprintln!("writing from {start_idx} to {end_idx}");

    cursor.seek_to_msg_idx(start_idx);

    while cursor.get_msg_idx() <= end_idx {
        let msg = cursor.read_raw_msg();
        let length = (msg.len() as i64).to_le_bytes();
        std::io::stdout().write(&length).unwrap();
        std::io::stdout().write_all(msg).unwrap();
    }
}

fn inspect_dump_msg(raw_msg: &[u8], idx: usize) {
    let value_result: Result<serde_json::Value, _> = serde_json::from_slice(raw_msg);
    match value_result {
        Ok(value) => {
            let fmt_value = serde_json::to_string_pretty(&value).expect("pretty please");
            // let fmt_value = value.to_string();
            println!("msg {idx}:\n\n{fmt_value}\n");
        }
        Err(_) => {
            let len = raw_msg.len();
            println!("msg {idx}: <binary> ({len} bytes)");
        }
    }
}

fn get_msg_type(maybe_value: &Option<serde_json::Value>) -> String {
    match maybe_value {
        None => "<binary>".to_string(),
        Some(value) => {
            let htype = value
                .as_object()
                .expect("all json messages should be objects")
                .get("htype");
            if let Some(htype_str) = htype && htype_str.is_string() {
                htype_str.as_str().expect("htype should be string here").to_string()
            } else {
                "<unknown>".to_string()
            }
        }
    }
}

fn get_summary(filename: &str) -> HashMap<String, usize> {
    let file = DumpRecordFile::new(&filename);
    let mut cursor = file.get_cursor();

    let mut msg_map = HashMap::<String, usize>::new();

    while !cursor.is_at_end() {
        let raw_msg = cursor.read_raw_msg();
        let value = try_parse(&raw_msg);
        let msg_type = get_msg_type(&value);
        msg_map.entry(msg_type).and_modify(|e| *e += 1).or_insert(1);
    }

    return msg_map;
}


fn inspect_print_summary(filename: &str) {
    let summary = get_summary(filenmae);

    println!("messages summary:");
    for (msg_type, count) in summary {
        println!("type {msg_type}: {count}");
    }
}

fn try_parse(raw_msg: &[u8]) -> Option<serde_json::Value> {
    let value_result: Result<serde_json::Value, _> = serde_json::from_slice(raw_msg);
    match value_result {
        Ok(value) => Some(value),
        Err(_) => None,
    }
}


pub fn action_inspect(
    cli: &Cli,
    head: Option<usize>,
    summary: bool,
) {

    let file = DumpRecordFile::new(&cli.filename);
    let mut cursor = file.get_cursor();

    match head {
        Some(head) => {
            for i in 0..head {
                let raw_msg = cursor.read_raw_msg();
                inspect_dump_msg(raw_msg, i);
            }
        }
        None => {
            let mut i = 0;
            while !cursor.is_at_end() {
                let raw_msg = cursor.read_raw_msg();
                inspect_dump_msg(raw_msg, i);
                i += 1;
            }
        }
    }

    if summary {
        inspect_print_summary(&cli.filename);
    }
}

fn write_raw_msg(msg: &[u8]) {
    let length = (msg.len() as i64).to_le_bytes();
    io::stdout().write(&length).unwrap();
    io::stdout().write_all(msg).unwrap();
}

fn write_serializable<T>(value: &T)
where
    T: Serialize,
{
    let binding = serde_json::to_string(&value).expect("serialization should not fail");
    let msg_raw = binding.as_bytes();
    write_raw_msg(&msg_raw);
}

pub fn action_repeat(
    cli: &Cli,
    repetitions: usize,
) {
    let file = DumpRecordFile::new(&cli.filename);
    let mut cursor = file.get_cursor();

    cursor.seek_to_first_header_of_type("dheader-1.0");
    let dheader = cursor.read_raw_msg();

    write_raw_msg(&dheader);

    // detector config
    let detector_config_msg = cursor.read_raw_msg();
    let _detector_config: DetectorConfig = serde_json::from_slice(detector_config_msg).unwrap();
    let mut detector_config_value: serde_json::Value =
        serde_json::from_slice::<serde_json::Value>(detector_config_msg)
            .unwrap()
            .to_owned();

    // XXX the heaer may lie about the number of images:
    let summary = get_summary(&cli.filename);
    let nimages = summary.get("<binary>").unwrap();
    let dest_num_images = nimages * cli.repetitions;

    let new_det_config = detector_config_value.as_object_mut().unwrap();
    new_det_config
        .entry("nimages")
        .and_modify(|v| *v = 1.into());
    new_det_config
        .entry("trigger_mode")
        .and_modify(|v| *v = "exte".to_string().into());
    new_det_config
        .entry("ntrigger")
        .and_modify(|v| *v = dest_num_images.into());

    write_serializable(&detector_config_value);

    let mut idx = 0;
    for _ in 0..cli.repetitions {
        let mut rep_cursor = file.get_cursor();
        rep_cursor.seek_to_first_header_of_type("dheader-1.0");
        let _dheader: DHeader = rep_cursor.read_and_deserialize().unwrap(); // discard dheader
        rep_cursor.read_raw_msg(); // discard detector config

        for _ in 0..*nimages {
            let mut dimage: DImage = rep_cursor
                .read_and_deserialize()
                .expect("failed to read dimage header");
            dimage.frame = idx;
            write_serializable(&dimage);

            let dimaged = rep_cursor.read_raw_msg();
            write_raw_msg(&dimaged);

            let image = rep_cursor.read_raw_msg();
            write_raw_msg(&image);

            // NOTE: we don't fake the timestamps (yet)
            let config = rep_cursor.read_raw_msg();
            write_raw_msg(&config);

            idx += 1;
        }
    }
}

fn action_sim(cli: &Cli, uri: String) {
    let mut sender = FrameSender::new(&cli.uri, &cli.filename);
    sender.send_headers();
    sender.send_frames();
    sender.send_footer();
}

pub fn main() {
    let cli = Cli::parse();

    match cli.action {
        Action::Cat { start_idx, end_idx } => action_cat(&cli, start_idx, end_idx),
        Action::Inspect { head, summary } => action_inspect(&cli, head, summary),
        Action::Repeat { repetitions } => action_repeat(&cli, repetitions),
        Action::Sim { uri } => action_sim(&cli, uri),
    }
}