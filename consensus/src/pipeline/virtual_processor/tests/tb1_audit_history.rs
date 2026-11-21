//! Replay the actual registered audit chain from genesis in separate native DBs.
//! Only public round inputs and raw blocks travel between processes.
use super::*;
use std::io::{BufRead, BufReader, BufWriter, Lines};

const READER: &str = "MISAKA_PRIVATE_CERTIFIED_AUDIT_READ";
pub(super) const OUTPUT: &str = "MISAKA_PRIVATE_CERTIFIED_AUDIT_WRITE";

#[derive(Serialize, Deserialize)]
pub(super) struct Header {
    genesis: Hash64,
    pub registered: Hash64,
    pub dealer_roots: [[Hash64; 4]; 2],
    pub signatures: [Vec<Vec<u8>>; 2],
    pub floor: Option<Hash64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custody: Option<[[Hash64; 3]; 2]>,
    tip: Hash64,
    count: usize,
}

pub(super) struct Reader {
    pub header: Header,
    lines: Lines<BufReader<File>>,
    count: usize,
}

impl Reader {
    pub fn open(genesis: Hash64) -> Option<Self> {
        let path = std::env::var_os(READER)?;
        let mut lines = BufReader::new(File::open(path).unwrap()).lines();
        let header: Header = serde_json::from_str(&lines.next().unwrap().unwrap()).unwrap();
        assert_eq!(header.genesis, genesis);
        assert!(header.count > 0);
        Some(Self { header, lines, count: 0 })
    }

    pub async fn next(&mut self, node: &mut Rig) -> Block {
        assert!(self.count < self.header.count);
        let raw: RpcRawBlock = serde_json::from_str(&self.lines.next().expect("complete audit history").unwrap()).unwrap();
        let block = Block::try_from(raw).unwrap();
        let parent = node.chain.sink();
        node.chain.ctx.simulated_time = node.chain.ctx.simulated_time.max(block.header.timestamp);
        stable_panel::arm(node);
        let status =
            node.api().validate_and_insert_block(block.clone()).virtual_state_task.await.expect("validate the recorded block");
        assert!(status.has_block_body());
        assert_eq!(node.chain.sink(), block.hash());
        assert_eq!(node.api().get_ghostdag_data(block.hash()).unwrap().selected_parent, parent);
        self.count += 1;
        block
    }

    pub async fn finish(&mut self, node: &mut Rig) {
        while self.count < self.header.count {
            self.next(node).await;
        }
        assert!(self.lines.next().is_none(), "unexpected blocks after declared tip");
        assert_eq!(node.chain.sink(), self.header.tip);
    }
}

pub(super) fn save(
    node: &Rig,
    path: &Path,
    registered: Hash64,
    floor: Option<Hash64>,
    custody: Option<[[Hash64; 3]; 2]>,
    roots: [[Hash64; 4]; 2],
    signatures: [Vec<Vec<u8>>; 2],
) {
    let tip = node.chain.sink();
    let blocks = node.blocks_through(tip);
    let mut out = BufWriter::new(File::create_new(path).unwrap());
    let header = Header {
        genesis: node.config.params.genesis.hash,
        registered,
        dealer_roots: roots,
        signatures,
        floor,
        custody,
        tip,
        count: blocks.len(),
    };
    serde_json::to_writer(&mut out, &header).unwrap();
    for block in blocks {
        out.write_all(b"\n").unwrap();
        serde_json::to_writer(&mut out, &RpcRawBlock::from(&block)).unwrap();
    }
    out.write_all(b"\n").unwrap();
    out.flush().unwrap();
    out.get_ref().sync_all().unwrap();
}

pub(super) fn verify_peers(path: &Path, dir: &Path, observation: &serde_json::Value) {
    const RESULT: &str = "MISAKA_PRIVATE_CERTIFIED_AUDIT_RESULT";
    if std::env::var_os(READER).is_some() {
        if let Some(result) = std::env::var_os(RESULT) {
            serde_json::to_writer(File::create_new(result).unwrap(), observation).unwrap();
        }
        return;
    }
    let mut children = Vec::new();
    for slot in 1..=2 {
        let result = dir.join(format!("verified-{slot}.json"));
        let child = Command::new(std::env::current_exe().unwrap())
            .args(["private_registered_audit_certified_frontier", "--ignored", "--nocapture", "--test-threads=1"])
            .env(READER, path)
            .env(RESULT, &result)
            .spawn()
            .unwrap();
        children.push((child, result));
    }
    for (mut child, result) in children {
        assert!(child.wait().unwrap().success(), "independent audit replay failed");
        let checked: serde_json::Value = serde_json::from_reader(File::open(result).unwrap()).unwrap();
        assert_eq!(&checked, observation, "native DBs differ for the registered audit");
    }
}
