//! Narrow byte-identity pins for the generic-dims CPU train step (collab
//! LANES/throughput-architecture-20261020/WIDE-BACKWARD-DESIGN.md v2.1,
//! section 4.1). Every CPU route runs three consecutive updates on the
//! runner-fixed net-8 model. Each update yields one digest over every
//! numerical field the step returns (each float by its exact bits), its
//! secondary results (JSON as canonical bytes) and the post-update state
//! digest. The effective GAE forward and exact-backward worker counts are
//! recorded beside each digest, so the preparation-worker pins also prove
//! that 1, 2 and 4 workers give one digest.
//!
//! The expected digests were captured by running this test on main's code
//! (ab747a83) before the refactor, per target, from CI. An empty table is
//! capture mode: the test prints every pin line and fails.

use super::*;
use crate::line_b_teacher_target_v1::{line_b_teacher_target_v1, LineBDivergenceV1};
use crate::native_flat_tensorizer_v3::{encoded_decision_view_v3, NativeFlatDecisionTensorV3};
use crate::native_flat_tensorizer_v4::{
    encoded_decision_view_v4, NativeFlatDecisionTensorV4, NativeFlatTensorizerV4,
};
use crate::native_policy_train_step_v1::retention_v1::{
    RetainedImitationResultV1, RetentionGroupV1, RetentionRowV1,
};
use crate::native_policy_value_net_v1::ForwardActivationModeV1;
use crate::sideboard_play_policy_v1::FreshLineageGenerationV1;

const UPDATES_V1: usize = 3;
const VC: f32 = 0.75;
const LR: f32 = 0.00003;
/// The no-state-digest columns of the state encoder input.
const DIGEST_COLUMNS_V1: std::ops::Range<usize> = 123..219;

// Exact pins are scoped to the pinned toolchain and target tuple, as the
// fixed-partition state witness is: platform libm may differ in last bits.
// Each line is `route update forward_workers backward_workers digest`.
#[cfg(all(target_arch = "x86_64", target_os = "windows", target_env = "msvc"))]
const EXPECTED_TARGET_V1: &str = "x86_64-pc-windows-msvc";
#[cfg(all(target_arch = "x86_64", target_os = "windows", target_env = "msvc"))]
const EXPECTED_PINS_V1: &str = r"
wide_state_sha256 0 0 0 e511fc39eb7ec7ce4615509f37c039beeb0242b4f2a90b933d43775023cbaec6
wide_state_sha256 1 0 0 68aa38635d338dfec6153366825bf1eb3164f2ae00cb0c186469440299749fed
terminal_contract_sequential 0 0 0 f82e80839d416cea5c045f08d00826432617e7249228d6cf802387bae968e90c
terminal_contract_sequential 1 0 0 4ae17ffc3cc320aaa300b85345d79582436676c0c88cca02be12e42794fced83
terminal_contract_sequential 2 0 0 3021ef504c74b11fd0c2b9a3a9d2ecabf5643bf9401db75232f9dd147d87affa
terminal_contract_packed_recompute_w4 0 0 0 8628576230d3c19f0d425899b255fbf9b6a3e3db994d37c16ca8ccc1cbd6246f
terminal_contract_packed_recompute_w4 1 0 0 fb287c03ec6b1c56426a2bacfcbc86854c3d615bceb45ea13da97919df255ed1
terminal_contract_packed_recompute_w4 2 0 0 a67fb1705a0fcfc663eab6a28e667c1ac5027a061c15e53693710d31902fcd54
terminal_contract_fixed_partition_w1 0 0 0 d42f369830587bfc07c95ea0e436c80e6efe2c21c28a9837cfb82035651793a9
terminal_contract_fixed_partition_w1 1 0 0 b26db1a3aa23be6355a249951c3ea839a45472ce5b4e4cb29ca49aff190f9826
terminal_contract_fixed_partition_w1 2 0 0 1e7a3ccc089bdfc9f5cb9a507d47eaf98dbc59ca995cd7a553004cc0db8bdeb6
terminal_contract_fixed_partition_w4 0 0 0 d42f369830587bfc07c95ea0e436c80e6efe2c21c28a9837cfb82035651793a9
terminal_contract_fixed_partition_w4 1 0 0 b26db1a3aa23be6355a249951c3ea839a45472ce5b4e4cb29ca49aff190f9826
terminal_contract_fixed_partition_w4 2 0 0 1e7a3ccc089bdfc9f5cb9a507d47eaf98dbc59ca995cd7a553004cc0db8bdeb6
terminal_v3_sequential 0 0 0 659c7820c4a088747b2af160acbb6e4c539695f32350cd20e566b8dc69d74fed
terminal_v3_sequential 1 0 0 98259926642fa93609f19517ebc047738182fc05cf3ee1c745c197052920e8ce
terminal_v3_sequential 2 0 0 a32c6f57484276f0fc541aa0e1ac82e7ddb08ff07275ab3ee3ad4cfeb6568925
gae_v3_preparation_w1 0 1 1 782dd0e2c6148649398fddf1f6fceb773fac10db64947ceec2271a53a8597721
gae_v3_preparation_w1 1 1 1 6985ec8491cd4daf4872d0e9b25b0cf19dd1dd1ba09e1454c66b344e9ec0b23b
gae_v3_preparation_w1 2 1 1 25289780d679a8cd25c5479e4f28f04abd873847383608a5bd7dd29e2ee26068
gae_v3_preparation_w2 0 2 2 782dd0e2c6148649398fddf1f6fceb773fac10db64947ceec2271a53a8597721
gae_v3_preparation_w2 1 2 2 6985ec8491cd4daf4872d0e9b25b0cf19dd1dd1ba09e1454c66b344e9ec0b23b
gae_v3_preparation_w2 2 2 2 25289780d679a8cd25c5479e4f28f04abd873847383608a5bd7dd29e2ee26068
gae_v3_preparation_w4 0 4 4 782dd0e2c6148649398fddf1f6fceb773fac10db64947ceec2271a53a8597721
gae_v3_preparation_w4 1 4 4 6985ec8491cd4daf4872d0e9b25b0cf19dd1dd1ba09e1454c66b344e9ec0b23b
gae_v3_preparation_w4 2 4 4 25289780d679a8cd25c5479e4f28f04abd873847383608a5bd7dd29e2ee26068
gae_v3_fixed_partition_w1 0 2 0 cd5be7042cfc10eb2bca02aaf12e56406c90a746ada61e0996e6d965b50d7fdd
gae_v3_fixed_partition_w1 1 2 0 5e3cefc55cdf0a36f7468001d4e860e06140c0c4b5f056378ae0af69400bf396
gae_v3_fixed_partition_w1 2 2 0 73d54e2356341a3229fa167e835f99010344fe2c456ddcc338177afa57c7c1a9
gae_v3_fixed_partition_w4 0 2 0 cd5be7042cfc10eb2bca02aaf12e56406c90a746ada61e0996e6d965b50d7fdd
gae_v3_fixed_partition_w4 1 2 0 5e3cefc55cdf0a36f7468001d4e860e06140c0c4b5f056378ae0af69400bf396
gae_v3_fixed_partition_w4 2 2 0 73d54e2356341a3229fa167e835f99010344fe2c456ddcc338177afa57c7c1a9
weighted_v3 0 0 0 fe5840da0b56a78dcb475cc65295015df014075ee83451154cfd73d03fcc43e5
weighted_v3 1 0 0 b97e6e74f103bd8bbdeda92bfe967faa975cbbde7e857aaae573f1889bb10c15
weighted_v3 2 0 0 b83af40347654eb27cab1cdc8b45543fe864facd94ed5763c60684aa3f8d4372
gae_v3_line_b 0 1 1 2e0d26f4b4c0eb886501b008fb2d9b35d256ea4c3197c2b3a93e0d99f35e971b
gae_v3_line_b 1 1 1 002e16d624ea562ea6cc054e78619c62cbe32fe156cf7c9c7579e804cff1e9b7
gae_v3_line_b 2 1 1 c3ddf3f1d1cca2cab880d5e79659d74c6d008503850ecc95e8434d103927e265
imitation_v4 0 1 0 4cc450cacaf56debaf8c85c861e74a2e64446aefee8cc74b6b3c266535d826e8
imitation_v4 1 1 0 29382fdb01983e3d06bd0ca832108b7a46812dc5bc1866dede6a76b67e6aa944
imitation_v4 2 1 0 62cd7b25d320b04b333d2d46905fbef90c90db5dc26cd44175d63c11d7d68b8b
gae_v3_external_auxiliary 0 1 1 9e21480d60880f0904238df36ceecf2d190b4f1cc744cf2fde8a3ff6c88dd414
gae_v3_external_auxiliary 1 1 1 1c411291eef4aaa01a06f89c0ca8b36c8da30deb32cc10313dbfe6e8853c3577
gae_v3_external_auxiliary 2 1 1 6a3ae34e6017e7bcd962ce0920fd8209888725a508b139047a9289f91ad44be2
gae_v3_recovery 0 1 0 93ae160c1595f9f5d30c527b0b6f4396b46eaffaeb66de7b296f051ac2384cba
gae_v3_recovery 1 1 0 25ab7ec11b6a4da70cdaa3565d0f12fdfc0bfb235945f244eda8b442ccadfd98
gae_v3_recovery 2 1 0 686b5f933685daddcd26c4cfa36713b271055fd6b2b2e1275105c3aab66fb8ea
retention_v4 0 1 0 bf307ec875fe5b651e89c9ff4e1db6935b04fa375cf2141dbc6f9830817e9ed0
retention_v4 1 1 0 9ef5a9867f7907c7d527128da6f2fc50480dd70f6ee2b750314bd936de5ddba1
retention_v4 2 1 0 0c9073c24d268dd626977e64c1a02ebb5036b72748411b5053929764f4b6feb2
gae_v3_e_supported 0 1 1 736f66091bc11bd48a1934495d34b2bcf9bda945b1e72ad86957ac24bb215b5e
gae_v3_e_supported 1 1 1 47c2a2dbc69e6b7873f3039bf982c01f31bda8a9987988a4f95ad1e4564fef2c
gae_v3_e_supported 2 1 1 5412d9dec6e335544c51cbc675906c7f38cbe18b3ce7441da68328afa340f384
gae_v3_head_only_mask 0 1 1 c6d63d5af3d19702bbcb69f5cc4ec88fb36bddd6f09872de7cced999d1cc7423
gae_v3_head_only_mask 1 1 1 0c32e07a1fbcf0a5e251bc88d9663c2d07be634ebe1598678d5785a5dee0ddc5
gae_v3_head_only_mask 2 1 1 6563a0f8f482b805658190e498ca478829d4d953a58485b45f5d89ef33c2a6c1
gae_v3_portable_math_w2 0 2 2 42ba9ff922b9a68d963a7e0c1cf0ef0fa69dac77447bb1662eee75a1f1abbdf4
gae_v3_portable_math_w2 1 2 2 2d94fcbec2095f3b4612cf778ce86604f6dea119503a2119b2419719907c32ac
gae_v3_portable_math_w2 2 2 2 d0d0aacd0c372037f7122a12732c8a7d99a177bbbd97db95752a682bc3590675
gae_v3_no_state_digest_w2 0 2 2 10910bba4a1dd52bc6a5e0b1b49bc26861577cad747eba0f07f29c2f256e3c8d
gae_v3_no_state_digest_w2 1 2 2 3e16df217c6d7d6183ab20464a053abdd9bedc8002c77d4ae0000879d5f1c085
gae_v3_no_state_digest_w2 2 2 2 e68fe30b4e0e64f51b1414453ad757579811080fadafd312f4eee3533923ce3c
";

#[cfg(all(target_arch = "x86_64", target_os = "linux", target_env = "gnu"))]
const EXPECTED_TARGET_V1: &str = "x86_64-unknown-linux-gnu";
#[cfg(all(target_arch = "x86_64", target_os = "linux", target_env = "gnu"))]
const EXPECTED_PINS_V1: &str = r"
wide_state_sha256 0 0 0 e511fc39eb7ec7ce4615509f37c039beeb0242b4f2a90b933d43775023cbaec6
wide_state_sha256 1 0 0 68aa38635d338dfec6153366825bf1eb3164f2ae00cb0c186469440299749fed
terminal_contract_sequential 0 0 0 fe14ec82e4dac8167feb73a654ecf4a889125f7f8959af88a86895f82e5b317c
terminal_contract_sequential 1 0 0 53eec23e1e2e89ebd435514abdddb273ccc5cd3fb4708ac37b90e0c9714c18fc
terminal_contract_sequential 2 0 0 6f935e93bedc53769136863df041d4b7333bd64bd5b37cb8b27e607b364b7ba7
terminal_contract_packed_recompute_w4 0 0 0 8ba544c89e26ac7bc20876e766ba60fac0821c395dee26e34eef9d7f53964840
terminal_contract_packed_recompute_w4 1 0 0 1f373b5a83849f2f4e674ee2f0e576b89134c5e558c12779e5a22384b4245f7a
terminal_contract_packed_recompute_w4 2 0 0 7b529f28a1bc51530a4311edc01209f919e189feee10a7e8c8de1b5caa02d83f
terminal_contract_fixed_partition_w1 0 0 0 d96b231d1a9684ae3d5121bb6c706502e140861265b58d84c7469e9e76795bac
terminal_contract_fixed_partition_w1 1 0 0 adad8f17d849771f0fb6f943bee924b51403102c98703d8f1e46b5810f9ab9f6
terminal_contract_fixed_partition_w1 2 0 0 90605a7c55cdebeeb7b0eed2e610b39e7b4b747eaa8f4810671b68e8efb83041
terminal_contract_fixed_partition_w4 0 0 0 d96b231d1a9684ae3d5121bb6c706502e140861265b58d84c7469e9e76795bac
terminal_contract_fixed_partition_w4 1 0 0 adad8f17d849771f0fb6f943bee924b51403102c98703d8f1e46b5810f9ab9f6
terminal_contract_fixed_partition_w4 2 0 0 90605a7c55cdebeeb7b0eed2e610b39e7b4b747eaa8f4810671b68e8efb83041
terminal_v3_sequential 0 0 0 44b05ee6b3102f5717c58278af7ed634a3c93eaebdaf192e97ada07fb1a894c2
terminal_v3_sequential 1 0 0 21b5d755db37657acf2f55849aea3b765aa7a925db15f7e69a198b038ddca41e
terminal_v3_sequential 2 0 0 7a78d292cd957a5ed21260162b6073a20fcf572e8682d208259e8b90c76fdb11
gae_v3_preparation_w1 0 1 1 ca7c06b098954f8e9e8d818712e20a720e87e982b27c9eec6b12cb6dbadb5585
gae_v3_preparation_w1 1 1 1 b783dca9ba60c3571f3d78951dd05aa573b21e42814f47d1329afe15b3040bb8
gae_v3_preparation_w1 2 1 1 8ec6bbd457beefeaf0ac20e08dced0883893e62aee3f02ebb47766f009a55035
gae_v3_preparation_w2 0 2 2 ca7c06b098954f8e9e8d818712e20a720e87e982b27c9eec6b12cb6dbadb5585
gae_v3_preparation_w2 1 2 2 b783dca9ba60c3571f3d78951dd05aa573b21e42814f47d1329afe15b3040bb8
gae_v3_preparation_w2 2 2 2 8ec6bbd457beefeaf0ac20e08dced0883893e62aee3f02ebb47766f009a55035
gae_v3_preparation_w4 0 4 4 ca7c06b098954f8e9e8d818712e20a720e87e982b27c9eec6b12cb6dbadb5585
gae_v3_preparation_w4 1 4 4 b783dca9ba60c3571f3d78951dd05aa573b21e42814f47d1329afe15b3040bb8
gae_v3_preparation_w4 2 4 4 8ec6bbd457beefeaf0ac20e08dced0883893e62aee3f02ebb47766f009a55035
gae_v3_fixed_partition_w1 0 2 0 1a100f4162837274b74c65c68352b3befd39dfeb52a42a608a1f84fd9b92ea8f
gae_v3_fixed_partition_w1 1 2 0 0fb934dfeed58108b162e96b166470a5dab5982a7c1379e453ec9db64b58ed71
gae_v3_fixed_partition_w1 2 2 0 f63d88bc942194746010f956a2beb4672a105b2619bc652a23ca87500f815d25
gae_v3_fixed_partition_w4 0 2 0 1a100f4162837274b74c65c68352b3befd39dfeb52a42a608a1f84fd9b92ea8f
gae_v3_fixed_partition_w4 1 2 0 0fb934dfeed58108b162e96b166470a5dab5982a7c1379e453ec9db64b58ed71
gae_v3_fixed_partition_w4 2 2 0 f63d88bc942194746010f956a2beb4672a105b2619bc652a23ca87500f815d25
weighted_v3 0 0 0 978d4679474487b92fe8c9530eba10a5e900b6dfe3c76a0a5db07cd8e76eef2b
weighted_v3 1 0 0 2ea9213f68e0488c10addf69baea51c30eba9eae0a8c867c64a57a16b0d35e0a
weighted_v3 2 0 0 b4f33613c982b17c2885fa18786e0075232e904ef902d88ad68becbd7d64b32a
gae_v3_line_b 0 1 1 8a91f8f01d4fa74e8bfcd753b3c99af83f1aaf6ad4f8b8110960cea7926d5d82
gae_v3_line_b 1 1 1 1426f86f7d4ae90671c137de4995b111ac4d44db57aa6d5161d5e9566f06346e
gae_v3_line_b 2 1 1 157ba447c1af8320f1b9bc55dbfafd085c47af8c3240c959a2668bf33e0a45ce
imitation_v4 0 1 0 5727be7ed10e07cf88dd2763240459e5328a429d96d2b2a8047700dbe11111cd
imitation_v4 1 1 0 0540cc493820f76467f847da1856a024c37a90f8a3c8d06acdcf2511c6aa3c14
imitation_v4 2 1 0 7bdd3d1333a3f732701628d0da49e4684d3b5be871864558a5a1a49e0dcd283d
gae_v3_external_auxiliary 0 1 1 e2a08e513bd5462195076b20c578e05ef8497ba3f8041e12395f3e2d6607755c
gae_v3_external_auxiliary 1 1 1 cc45b6b9ed3e78fa4f29e12f194d6a9900d7a2988526b2956d001f7f30d8ce17
gae_v3_external_auxiliary 2 1 1 bb66735b012ddb093b66a627caff56f151da169ec8465a4316a4138c62ee8267
gae_v3_recovery 0 1 0 a6748745826665e527a0896308eb5830b494765ca9f6d3b84140506400837e44
gae_v3_recovery 1 1 0 5e0f71ed7499b9a47f9d64f5e6228e1c1a420d044e844d9d5625eba03aa3e398
gae_v3_recovery 2 1 0 7086d0ebfa79cb65a2859b0b6da708515ecba57524fa8fc0096238a89fa83eff
retention_v4 0 1 0 69312188aa7ff21404997b63d092c0d80aa30821a0a31d2400ad4b4a9c5b1a7f
retention_v4 1 1 0 0e4930fd1354e973d32e06e9365df9113dd265959a095f7249a461797e264333
retention_v4 2 1 0 720defad8176599f5a6efb85f8708d9feca421cd31624b575936045048c12ee0
gae_v3_e_supported 0 1 1 ae5e35844676ad1ba4836994376c51108e5a2f6bf586d85aca5b85222c945c37
gae_v3_e_supported 1 1 1 8cf53bcc9a6b57fbcc7a0a2c18041053f6fa09b8d21b58c3623aade0520f1364
gae_v3_e_supported 2 1 1 d676b8812e9024a84f4b3e88614dfaed1940abc671253c3840866c94b6bde5b5
gae_v3_head_only_mask 0 1 1 1fed08572c86415402ebdadd93fdfe9d378772b4177f1306c67046ef6d01569d
gae_v3_head_only_mask 1 1 1 ac441ca154252e7364aedabe466298cbc81a74ada71891f58afc4d284daca779
gae_v3_head_only_mask 2 1 1 4c4f3769098fb780558276e1d2aac48d0c76cc8ad51e89c8fb02ce356989796e
gae_v3_portable_math_w2 0 2 2 42ba9ff922b9a68d963a7e0c1cf0ef0fa69dac77447bb1662eee75a1f1abbdf4
gae_v3_portable_math_w2 1 2 2 2d94fcbec2095f3b4612cf778ce86604f6dea119503a2119b2419719907c32ac
gae_v3_portable_math_w2 2 2 2 d0d0aacd0c372037f7122a12732c8a7d99a177bbbd97db95752a682bc3590675
gae_v3_no_state_digest_w2 0 2 2 96c54d5b26582005f973d9c5fcdec0ae6282476ccf1db085a7b8dc6b376c73e0
gae_v3_no_state_digest_w2 1 2 2 a1794c850fb51d6d3ccc9b0ceb5ccdfb1136d111d51845a2c4e79b5d182f5556
gae_v3_no_state_digest_w2 2 2 2 647e474f46586a0d007114c288dbe3f23a4d13ccaa8002bca745ef8d6f0d7e35
";

// Unknown targets fail closed in capture mode.
#[cfg(not(any(
    all(target_arch = "x86_64", target_os = "windows", target_env = "msvc"),
    all(target_arch = "x86_64", target_os = "linux", target_env = "gnu")
)))]
const EXPECTED_TARGET_V1: &str = "an unreviewed target";
#[cfg(not(any(
    all(target_arch = "x86_64", target_os = "windows", target_env = "msvc"),
    all(target_arch = "x86_64", target_os = "linux", target_env = "gnu")
)))]
const EXPECTED_PINS_V1: &str = "";

/// SHA-256 over a length-prefixed canonical encoding: every count as a
/// little-endian u64 and every float by its exact bits. It binds the update
/// index but not the route name, so routes that must agree (worker counts)
/// compare digests directly.
struct PinDigestV1(Sha256);

impl PinDigestV1 {
    fn new(update: usize) -> Self {
        let mut digest = Self(Sha256::new());
        digest.bytes(b"mtg-kernel/narrow-dims-refactor-bit-pin/v1");
        digest.count(update);
        digest
    }

    fn bytes(&mut self, bytes: &[u8]) {
        self.0.update((bytes.len() as u64).to_le_bytes());
        self.0.update(bytes);
    }

    fn count(&mut self, value: usize) {
        self.0.update((value as u64).to_le_bytes());
    }

    fn f32(&mut self, value: f32) {
        self.0.update(value.to_bits().to_le_bytes());
    }

    fn f64(&mut self, value: f64) {
        self.0.update(value.to_bits().to_le_bytes());
    }

    fn json(&mut self, value: &serde_json::Value) {
        self.bytes(&serde_json::to_vec(value).expect("pinned JSON serializes"));
    }

    fn optional_json(&mut self, value: Option<&serde_json::Value>) {
        match value {
            None => self.count(0),
            Some(value) => {
                self.count(1);
                self.json(value);
            }
        }
    }

    fn tensors(&mut self, tensors: &[NativeNamedParameterV1]) {
        self.count(tensors.len());
        for tensor in tensors {
            self.bytes(tensor.name.as_bytes());
            self.count(tensor.shape.len());
            for &dimension in &tensor.shape {
                self.count(dimension);
            }
            let bits = tensor
                .values
                .iter()
                .flat_map(|value| value.to_bits().to_le_bytes())
                .collect::<Vec<_>>();
            self.bytes(&bits);
        }
    }

    fn gauge(&mut self, gauge: &NativeScorerBiasGaugeRecordV1) {
        self.bytes(gauge.parameter_name.as_bytes());
        self.count(gauge.substep_count);
        self.count(gauge.total_action_count);
        self.count(gauge.max_action_count);
        self.f64(gauge.sum_abs_policy_coefficients);
        self.count(gauge.substep_bounds.len());
        for bound in &gauge.substep_bounds {
            self.count(bound.action_count);
            self.f64(bound.abs_policy_coefficient);
            self.count(bound.gamma_operation_count);
            self.f64(bound.gamma);
            self.f64(bound.bound_component);
        }
        self.f64(gauge.per_substep_bound_sum);
        self.f64(gauge.cross_substep_bound);
        self.f32(gauge.raw_gradient_residual);
        self.f64(gauge.derived_absolute_bound);
        self.f64(gauge.high_precision_residual);
        self.f32(gauge.canonical_gradient);
        self.0.update(gauge.parameter_before_bits.to_le_bytes());
        self.0.update(gauge.parameter_after_bits.to_le_bytes());
    }

    fn result(&mut self, result: &NativePolicyTrainStepResultV1) {
        self.f32(result.policy_sum);
        self.f32(result.value_sum);
        self.f32(result.loss);
        self.0.update(result.adam_step.to_le_bytes());
        self.count(result.selected_outputs.len());
        for output in &result.selected_outputs {
            self.count(output.group_index);
            self.count(output.substep_index);
            self.count(output.selected_action_index);
            self.f32(output.selected_logit);
            self.f32(output.value);
            self.f32(output.selected_log_probability);
        }
        self.count(result.physical_terms.len());
        for term in &result.physical_terms {
            self.f32(term.joint_log_probability);
            self.f32(term.value);
            self.0.update(term.terminal_return.to_le_bytes());
            self.0.update(term.substep_count.to_le_bytes());
        }
        self.tensors(&result.gradients);
        self.gauge(&result.scorer_bias_gauge);
    }

    fn auxiliary(&mut self, auxiliary: &LineBAuxiliaryResultV1) {
        self.json(&auxiliary.gradient_components);
        self.count(auxiliary.divergences_before.len());
        for divergence in &auxiliary.divergences_before {
            match divergence {
                None => self.count(0),
                Some(value) => {
                    self.count(1);
                    self.f64(*value);
                }
            }
        }
        self.f64(auxiliary.auxiliary_loss);
        self.f64(auxiliary.auxiliary_head_l2);
        self.f64(auxiliary.ordinary_head_l2);
        self.f32(auxiliary.auxiliary_bias_residual);
        self.optional_json(auxiliary.cap.as_ref());
        self.optional_json(auxiliary.adam_steps.as_ref());
    }

    fn retained(&mut self, result: &RetainedImitationResultV1) {
        self.f32(result.teacher_loss);
        self.f32(result.retention_kl);
        self.f32(result.weighted_retention_loss);
        self.f32(result.loss);
        self.0.update(result.adam_step.to_le_bytes());
        self.tensors(&result.gradients);
        self.gauge(&result.teacher_gauge);
        match &result.retention_gauge {
            None => self.count(0),
            Some(gauge) => {
                self.count(1);
                self.gauge(gauge);
            }
        }
    }

    fn finish(self) -> String {
        hex_sha256(self.0.finalize().into())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PinV1 {
    route: &'static str,
    update: usize,
    forward_workers: usize,
    backward_workers: usize,
    digest: String,
}

impl PinV1 {
    fn line_v1(&self) -> String {
        format!(
            "{} {} {} {} {}",
            self.route, self.update, self.forward_workers, self.backward_workers, self.digest
        )
    }
}

/// Three consecutive updates of one route from `state`; each digest also
/// covers the post-update state digest.
fn updates_v1(
    route: &'static str,
    mut state: NativePolicyValueTrainStateV1,
    mut update: impl FnMut(&mut NativePolicyValueTrainStateV1, &mut PinDigestV1),
) -> Vec<PinV1> {
    (0..UPDATES_V1)
        .map(|index| {
            take_effective_update_workers_for_test_v1();
            let mut digest = PinDigestV1::new(index);
            update(&mut state, &mut digest);
            let (forward_workers, backward_workers) = take_effective_update_workers_for_test_v1();
            digest.bytes(&state.state_sha256_v1().expect("updated state is valid"));
            PinV1 {
                route,
                update: index,
                forward_workers,
                backward_workers,
                digest: digest.finish(),
            }
        })
        .collect()
}

fn model_v1() -> NativePolicyValueNetV1 {
    NativePolicyValueNetV1::runner_fixed_v1(NativePolicyValueModelConfigV1::contract_v1()).unwrap()
}

fn state_v1() -> NativePolicyValueTrainStateV1 {
    NativePolicyValueTrainStateV1::new_v1(model_v1()).unwrap()
}

/// Two V3 tensors from the real Map choice and the outputs a model records
/// for them (the `gae_v1` unit fixture).
struct V3FixtureV1 {
    tensors: [NativeFlatDecisionTensorV3; 2],
    logits: [Vec<u32>; 2],
    values: [u32; 2],
}

impl V3FixtureV1 {
    fn tensors_v1() -> [NativeFlatDecisionTensorV3; 2] {
        let first = real_map_training_tensor_v3();
        let mut second = first.clone();
        second.common.state[0] += 0.125;
        [first, second]
    }

    fn capture_v1(
        model: &NativePolicyValueNetV1,
        tensors: [NativeFlatDecisionTensorV3; 2],
        mode: ForwardActivationModeV1,
    ) -> Self {
        let outputs = tensors.each_ref().map(|tensor| {
            model
                .forward_feature_transfer_v3_mode(encoded_decision_view_v3(tensor), mode)
                .unwrap()
        });
        assert!(outputs.iter().all(|output| output.logits.len() >= 2));
        Self {
            logits: outputs
                .each_ref()
                .map(|output| output.logits.iter().map(|v| v.to_bits()).collect()),
            values: outputs.each_ref().map(|output| output.value.to_bits()),
            tensors,
        }
    }

    fn of_v1(model: &NativePolicyValueNetV1) -> Self {
        Self::capture_v1(model, Self::tensors_v1(), ForwardActivationModeV1::LibmTanh)
    }

    fn substep_v1(&self, tensor: usize, selected: usize) -> NativePolicySubstepV1<'_> {
        NativePolicySubstepV1 {
            forward: NativePolicyForwardInputV1::Encoded(Box::new(encoded_decision_view_v3(
                &self.tensors[tensor],
            ))),
            selected_action_index: selected,
            expected_raw_action_logit_bits: self.logits[tensor].as_slice(),
            expected_value_bits: self.values[tensor],
        }
    }

    /// Two groups, of two substeps and one.
    fn small_v1(&self) -> Vec<Vec<NativePolicySubstepV1<'_>>> {
        vec![
            vec![self.substep_v1(0, 0), self.substep_v1(1, 1)],
            vec![self.substep_v1(1, self.logits[1].len() - 1)],
        ]
    }

    /// 200 groups of one to three substeps (399 substeps), so four forward
    /// workers and four exact-backward workers engage.
    fn many_v1(&self) -> Vec<Vec<NativePolicySubstepV1<'_>>> {
        (0..200)
            .map(|group| {
                (0..1 + group % 3)
                    .map(|substep| {
                        let tensor = (group + substep) % 2;
                        let selected = (group * 7 + substep) % self.logits[tensor].len();
                        self.substep_v1(tensor, selected)
                    })
                    .collect()
            })
            .collect()
    }
}

fn physical_groups_v1<'a>(
    steps: &'a [Vec<NativePolicySubstepV1<'a>>],
) -> Vec<NativePolicyPhysicalDecisionV1<'a>> {
    steps
        .iter()
        .enumerate()
        .map(|(group, substeps)| NativePolicyPhysicalDecisionV1 {
            substeps,
            terminal_return: [1, -1, 0][group % 3],
            baseline_bits: 0,
        })
        .collect()
}

/// Per-group value targets and advantages, not reducible to the terminal
/// objective.
fn gae_targets_v1(group_count: usize) -> (Vec<f32>, Vec<f32>) {
    let targets = (0..group_count)
        .map(|group| 0.25 - 0.01 * group as f32)
        .collect();
    let advantages = (0..group_count)
        .map(|group| {
            let sign = if group % 2 == 0 { 0.5 } else { -0.375 };
            sign / (1.0 + group as f32)
        })
        .collect();
    (targets, advantages)
}

fn line_b_target_v1(fixture: &V3FixtureV1) -> crate::line_b_teacher_target_v1::LineBSoftmaxV1 {
    let logits: Vec<f64> = fixture.logits[1]
        .iter()
        .map(|bits| f64::from(f32::from_bits(*bits)))
        .collect();
    let returns: Vec<f64> = (0..logits.len()).map(|i| [1.0, 0.0, -1.0][i % 3]).collect();
    line_b_teacher_target_v1(&logits, &returns, 0.25).unwrap()
}

/// The terminal objective on the golden fixture's contract views.
fn terminal_contract_v1(
    route: &'static str,
    run: impl Fn(
        &mut NativePolicyValueTrainStateV1,
        &[NativePolicyPhysicalDecisionV1<'_>],
        f32,
        f32,
    ) -> NativePolicyTrainStepResultV1,
) -> Vec<PinV1> {
    let (forward, golden) = fixtures();
    let step = &golden.steps[2];
    updates_v1(route, state_v1(), |state, digest| {
        let expected = expected_outputs_for_step(state.model_v1(), &forward, step);
        let substeps = substeps_for_step(&forward, step, &expected);
        let groups = groups_for_step(step, &substeps);
        let result = run(
            state,
            &groups[..8],
            golden.value_coefficient,
            golden.optimizer.learning_rate,
        );
        digest.result(&result);
    })
}

/// The terminal objective with packed tapes and four recompute workers.
fn terminal_packed_v1() -> Vec<PinV1> {
    let (forward, golden) = fixtures();
    let step = &golden.steps[2];
    updates_v1(
        "terminal_contract_packed_recompute_w4",
        state_v1(),
        |state, digest| {
            let expected = expected_outputs_for_step(state.model_v1(), &forward, step);
            let builder =
                NativePolicyPackedForwardBuilderV1::from_model_v1(state.model_v1()).unwrap();
            let tapes = packed_tapes_for_step(&builder, &forward, step);
            let substeps = packed_substeps_for_step(&forward, step, &expected, &tapes);
            let base = groups_for_step(step, &substeps);
            let base = &base[..8];
            let substep_count = base.iter().map(|group| group.substeps.len()).sum::<usize>();
            let groups = (0..PACKED_RECOMPUTE_MIN_SUBSTEPS_V1.div_ceil(substep_count))
                .flat_map(|_| base.iter().copied())
                .collect::<Vec<_>>();
            let control = PackedRecomputeTestControlV1::new_v1(None, Vec::new());
            let result = {
                let _guard = install_packed_recompute_test_control_v1(Arc::clone(&control));
                state
                    .train_step_with_recompute_workers_v1(
                        &groups,
                        golden.value_coefficient,
                        golden.optimizer.learning_rate,
                        PACKED_RECOMPUTE_MAX_WORKERS_V1,
                    )
                    .unwrap()
            };
            assert_eq!(control.spawn_counts_v1(), (4, 4, 0));
            digest.result(&result);
        },
    )
}

/// The RT1 CPU GAE route (`train_step_gae_feature_transfer_profiled_v1`) on
/// the 399-substep fixture.
fn gae_profiled_v1(
    route: &'static str,
    mut state: NativePolicyValueTrainStateV1,
    mode: ForwardActivationModeV1,
    tensors: fn() -> [NativeFlatDecisionTensorV3; 2],
    backward_worker_limit: Option<usize>,
    forward_workers: usize,
) -> Vec<PinV1> {
    if !matches!(mode, ForwardActivationModeV1::LibmTanh) {
        state.set_training_math_v1(TrainingMathV1::PortableV1);
    }
    updates_v1(route, state, |state, digest| {
        let fixture = V3FixtureV1::capture_v1(state.model_v1(), tensors(), mode);
        let steps = fixture.many_v1();
        let groups = physical_groups_v1(&steps);
        let (targets, advantages) = gae_targets_v1(groups.len());
        let result = state
            .train_step_gae_feature_transfer_profiled_v1(
                FreshLineageGenerationV1::V3,
                &groups,
                &targets,
                &advantages,
                VC,
                LR,
                backward_worker_limit,
                forward_workers,
                &mut NativeTrainingPhaseRecorderV1::disabled_v1(),
            )
            .unwrap();
        digest.result(&result);
    })
}

/// An ordinary update on the two-group V3 fixture.
fn small_v3_v1(
    route: &'static str,
    run: impl Fn(
        &mut NativePolicyValueTrainStateV1,
        &V3FixtureV1,
        &[NativePolicyPhysicalDecisionV1<'_>],
        &mut PinDigestV1,
    ),
) -> Vec<PinV1> {
    updates_v1(route, state_v1(), |state, digest| {
        let fixture = V3FixtureV1::of_v1(state.model_v1());
        let steps = fixture.small_v1();
        let groups = physical_groups_v1(&steps);
        run(state, &fixture, &groups, digest);
    })
}

/// The real Map choice as a V4 tensor (the `retention_v1` unit fixture).
fn v4_tensor_v1() -> NativeFlatDecisionTensorV4 {
    use crate::flat_policy_v2::{FlatScoringDecisionViewV2, FlatScoringOwnedBuffersV2};
    use crate::flat_policy_v4::{FlatDecisionEncoderV4, FlatScoringDecisionViewV4};
    use crate::rl_session::{FastActorResponseV1, FastActorSessionV1};
    let session = FastActorSessionV1::from_v3_fixture_state(
        crate::policy_observation_v6::tests::map_choice_state().0,
    );
    let FastActorResponseV1::Decision(decision) = session.current_response() else {
        panic!("missing Map choice")
    };
    let (
        mut objects,
        mut relations,
        mut subtypes,
        mut uses,
        mut goads,
        mut dungeons,
        mut effects,
        mut paths,
        mut actions,
        mut refs,
    ) = (
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
    );
    let encoded = session
        .encode_current_flat_scoring_decision_owned_v4(
            decision,
            &mut FlatDecisionEncoderV4::default(),
            &mut FlatScoringOwnedBuffersV2 {
                objects: &mut objects,
                relations: &mut relations,
                object_subtypes: &mut subtypes,
                ability_uses: &mut uses,
                goads: &mut goads,
                completed_dungeons: &mut dungeons,
                effect_subtype_changes: &mut effects,
                context_path_elements: &mut paths,
                actions: &mut actions,
                action_refs: &mut refs,
            },
        )
        .unwrap();
    let common = FlatScoringDecisionViewV2::new(
        &encoded.globals,
        &objects,
        &relations,
        &subtypes,
        &uses,
        &goads,
        &dungeons,
        &effects,
        &paths,
        &actions,
        &refs,
    );
    let mut tensor = NativeFlatDecisionTensorV4::default();
    NativeFlatTensorizerV4::default()
        .fill(
            FlatScoringDecisionViewV4::new(common, &encoded.extensions),
            &mut tensor,
        )
        .unwrap();
    tensor
}

/// The V4 tensor, the outputs a model records for it and frozen parent
/// logits for retention rows.
struct V4FixtureV1 {
    tensor: NativeFlatDecisionTensorV4,
    logits: Vec<u32>,
    value: u32,
    parent: Vec<f32>,
}

impl V4FixtureV1 {
    fn of_v1(model: &NativePolicyValueNetV1) -> Self {
        let tensor = v4_tensor_v1();
        let output = model
            .forward_feature_transfer_v4(encoded_decision_view_v4(&tensor))
            .unwrap();
        assert!(output.logits.len() >= 2);
        Self {
            parent: output
                .logits
                .iter()
                .enumerate()
                .map(|(index, logit)| logit + 0.7 * index as f32)
                .collect(),
            logits: output.logits.iter().map(|v| v.to_bits()).collect(),
            value: output.value.to_bits(),
            tensor,
        }
    }

    fn substep_v1(&self, selected: usize) -> NativePolicySubstepV1<'_> {
        NativePolicySubstepV1 {
            forward: NativePolicyForwardInputV1::Encoded(Box::new(encoded_decision_view_v4(
                &self.tensor,
            ))),
            selected_action_index: selected,
            expected_raw_action_logit_bits: &self.logits,
            expected_value_bits: self.value,
        }
    }

    /// Two teaching groups, of one substep each.
    fn teaching_v1(&self) -> [Vec<NativePolicySubstepV1<'_>>; 2] {
        [
            vec![self.substep_v1(0)],
            vec![self.substep_v1(self.logits.len() - 1)],
        ]
    }

    fn retention_v1(&self) -> RetentionRowV1<'_> {
        RetentionRowV1 {
            encoded: encoded_decision_view_v4(&self.tensor),
            expected_current_logits: &self.logits,
            expected_current_value: self.value,
            parent_logits: &self.parent,
        }
    }
}

fn teaching_groups_v1<'a>(
    steps: &'a [Vec<NativePolicySubstepV1<'a>>; 2],
) -> [NativePolicyPhysicalDecisionV1<'a>; 2] {
    steps
        .each_ref()
        .map(|substeps| NativePolicyPhysicalDecisionV1 {
            substeps,
            terminal_return: 1,
            baseline_bits: 0,
        })
}

/// A runner-fixed state under the no-state-digest contract: the digest
/// columns of the state encoder weight are +0.0 and its bias holds no -0.0.
fn no_state_digest_state_v1() -> NativePolicyValueTrainStateV1 {
    let mut snapshot = state_v1().snapshot_v1().unwrap();
    for row in snapshot.parameters[STATE_FIRST_WEIGHT]
        .values
        .chunks_exact_mut(STATE_ENCODER_INPUT)
    {
        row[DIGEST_COLUMNS_V1].fill(0.0);
    }
    for value in &mut snapshot.parameters[STATE_FIRST_BIAS].values {
        if value.to_bits() == 0x8000_0000 {
            *value = 0.0;
        }
    }
    let mut state = NativePolicyValueTrainStateV1::from_snapshot_v1(model_v1(), &snapshot).unwrap();
    state.install_no_state_digest_v1(DIGEST_COLUMNS_V1).unwrap();
    state
}

fn no_state_digest_tensors_v1() -> [NativeFlatDecisionTensorV3; 2] {
    let mut tensors = V3FixtureV1::tensors_v1();
    for tensor in &mut tensors {
        tensor.common.state[DIGEST_COLUMNS_V1].fill(0.0);
    }
    tensors
}

/// The wide train state's digests before the refactor (design section 5.5):
/// a new runner-fixed wide state, and an owned snapshot of it with a later
/// Adam step and nonzero moments. Platform-free: no libm is involved.
fn wide_state_hash_pins_v1() -> Vec<PinV1> {
    let model = NativePolicyValueNetWideV1::runner_fixed_wide_v1(
        NativePolicyValueModelConfigV1::contract_wide_v1(),
    )
    .unwrap();
    let state = NativePolicyValueTrainStateWideV1::new_wide_v1(model).unwrap();
    let mut snapshot = state.snapshot_v1().unwrap();
    snapshot.adam_step = 7;
    for (index, (first, second)) in snapshot
        .first_moments
        .iter_mut()
        .zip(&mut snapshot.second_moments)
        .enumerate()
    {
        if index == CARD_EMBEDDING || index == SCORER_SECOND_BIAS {
            continue;
        }
        first.values[0] = 0.25 / (1.0 + index as f32);
        second.values[0] = 0.125 / (1.0 + index as f32);
    }
    [
        state.state_sha256_v1().unwrap(),
        wide_owned_train_snapshot_state_sha256_v1(&snapshot).unwrap(),
    ]
    .into_iter()
    .enumerate()
    .map(|(update, digest)| PinV1 {
        route: "wide_state_sha256",
        update,
        forward_workers: 0,
        backward_workers: 0,
        digest: hex_sha256(digest),
    })
    .collect()
}

fn all_pins_v1() -> Vec<PinV1> {
    let mut pins = wide_state_hash_pins_v1();
    pins.extend(terminal_contract_v1(
        "terminal_contract_sequential",
        |state, groups, vc, lr| state.train_step_v1(groups, vc, lr).unwrap(),
    ));
    pins.extend(terminal_packed_v1());
    for (route, workers) in [
        ("terminal_contract_fixed_partition_w1", 1),
        ("terminal_contract_fixed_partition_w4", 4),
    ] {
        pins.extend(terminal_contract_v1(route, |state, groups, vc, lr| {
            state
                .train_step_with_fixed_partition_parallel_backward_v1(groups, vc, lr, 1, workers)
                .unwrap()
        }));
    }
    pins.extend(small_v3_v1(
        "terminal_v3_sequential",
        |state, _, groups, digest| {
            digest.result(
                &state
                    .train_step_feature_transfer_v3(groups, VC, LR)
                    .unwrap(),
            );
        },
    ));
    for (route, workers) in [
        ("gae_v3_preparation_w1", 1),
        ("gae_v3_preparation_w2", 2),
        ("gae_v3_preparation_w4", 4),
    ] {
        pins.extend(gae_profiled_v1(
            route,
            state_v1(),
            ForwardActivationModeV1::LibmTanh,
            V3FixtureV1::tensors_v1,
            None,
            workers,
        ));
    }
    for (route, workers) in [
        ("gae_v3_fixed_partition_w1", 1),
        ("gae_v3_fixed_partition_w4", 4),
    ] {
        pins.extend(gae_profiled_v1(
            route,
            state_v1(),
            ForwardActivationModeV1::LibmTanh,
            V3FixtureV1::tensors_v1,
            Some(workers),
            2,
        ));
    }
    pins.extend(small_v3_v1("weighted_v3", |state, _, groups, digest| {
        digest.result(
            &state
                .train_step_weighted_feature_transfer_v3(groups, &[0.375, 0.625], VC, LR)
                .unwrap(),
        );
    }));
    pins.extend(small_v3_v1(
        "gae_v3_line_b",
        |state, fixture, groups, digest| {
            let input = LineBAuxiliaryInputV1 {
                scheduled_count: None,
                direction: LineBDivergenceV1::Forward,
                coefficient: 0.7,
                roots: vec![
                    LineBAuxiliaryRootV1 {
                        group_index: 0,
                        substep_index: 1,
                        target: Some(line_b_target_v1(fixture)),
                    },
                    LineBAuxiliaryRootV1 {
                        group_index: 1,
                        substep_index: 0,
                        target: None,
                    },
                ],
            };
            let (targets, advantages) = gae_targets_v1(groups.len());
            let config = state.model_v1().feature_transfer_config_v3();
            let (result, auxiliary) = state
                .train_step_gae_line_b_v1(
                    groups,
                    &targets,
                    &advantages,
                    VC,
                    LR,
                    None,
                    config,
                    &input,
                )
                .unwrap();
            digest.result(&result);
            digest.auxiliary(&auxiliary);
        },
    ));
    pins.extend(updates_v1("imitation_v4", state_v1(), |state, digest| {
        let fixture = V4FixtureV1::of_v1(state.model_v1());
        let steps = fixture.teaching_v1();
        let groups = teaching_groups_v1(&steps);
        digest.result(
            &state
                .train_step_terminal_winner_imitation_v4(&groups, LR, 4)
                .unwrap(),
        );
    }));
    pins.extend(small_v3_v1(
        "gae_v3_external_auxiliary",
        |state, fixture, groups, digest| {
            let target = line_b_target_v1(fixture);
            let external = ExternalAuxiliaryInputV1 {
                direction: LineBDivergenceV1::Forward,
                coefficient: 0.7,
                denominator: 2,
                cap: Some(1.0e-3),
                step_telemetry: true,
                rows: (0..2)
                    .map(|_| ExternalAuxiliaryRowV1 {
                        encoded: encoded_decision_view_v3(&fixture.tensors[1]),
                        target: &target,
                    })
                    .collect(),
            };
            let (targets, advantages) = gae_targets_v1(groups.len());
            let (result, auxiliary) = state
                .train_step_gae_external_auxiliary_v1(
                    FreshLineageGenerationV1::V3,
                    groups,
                    &targets,
                    &advantages,
                    VC,
                    LR,
                    None,
                    &external,
                )
                .unwrap();
            digest.result(&result);
            digest.auxiliary(&auxiliary);
        },
    ));
    pins.extend(small_v3_v1(
        "gae_v3_recovery",
        |state, _, groups, digest| {
            let (targets, advantages) = gae_targets_v1(groups.len());
            // The first update also records the diagnostics.
            let update = state.adam_step_v1() + 1;
            let (result, report) = state
                .train_step_gae_recovery_v1(
                    FreshLineageGenerationV1::V3,
                    groups,
                    &targets,
                    &advantages,
                    VC,
                    LR,
                    0.01,
                    update,
                    true,
                )
                .unwrap();
            digest.result(&result);
            digest.json(&report);
        },
    ));
    pins.extend(updates_v1("retention_v4", state_v1(), |state, digest| {
        let fixture = V4FixtureV1::of_v1(state.model_v1());
        let steps = fixture.teaching_v1();
        let groups = teaching_groups_v1(&steps);
        let two = [fixture.retention_v1(), fixture.retention_v1()];
        let one = [fixture.retention_v1()];
        let retained = [
            RetentionGroupV1 { rows: &two },
            RetentionGroupV1 { rows: &one },
        ];
        digest.retained(
            &state
                .train_step_retained_imitation_v4(&groups, &retained, 0.75, LR, 4)
                .unwrap(),
        );
    }));
    pins.extend(small_v3_v1(
        "gae_v3_e_supported",
        |state, fixture, groups, digest| {
            let input = ESupportedPolicyInputV1 {
                scheduled: 2,
                treatment: true,
                rows: vec![ESupportedPolicyRowV1 {
                    group_index: 0,
                    substep_index: 1,
                    legal_width: fixture.logits[1].len(),
                    target_index: 0,
                }],
            };
            let (targets, advantages) = gae_targets_v1(groups.len());
            let (result, report) = state
                .train_step_gae_e_supported_v1(
                    FreshLineageGenerationV1::V3,
                    groups,
                    &targets,
                    &advantages,
                    VC,
                    LR,
                    None,
                    &input,
                )
                .unwrap();
            digest.result(&result);
            digest.optional_json(report.as_ref());
        },
    ));
    pins.extend(small_v3_v1(
        "gae_v3_head_only_mask",
        |state, _, groups, digest| {
            let (targets, advantages) = gae_targets_v1(groups.len());
            let before = state.snapshot_v1().unwrap();
            let mask = HeadOnlyMaskV1::for_snapshot_v1(&before).unwrap();
            let result = state
                .train_step_gae_feature_transfer_v3(groups, &targets, &advantages, VC, LR)
                .unwrap();
            let mut after = state.snapshot_v1().unwrap();
            mask.restore_frozen_v1(&before, &mut after).unwrap();
            state.replace_snapshot_v1(&after).unwrap();
            digest.result(&result);
            digest.bytes(&mask.frozen_sha256_v1(&after));
        },
    ));
    pins.extend(gae_profiled_v1(
        "gae_v3_portable_math_w2",
        state_v1(),
        ForwardActivationModeV1::KernelDeterministicTanh,
        V3FixtureV1::tensors_v1,
        None,
        2,
    ));
    pins.extend(gae_profiled_v1(
        "gae_v3_no_state_digest_w2",
        no_state_digest_state_v1(),
        ForwardActivationModeV1::LibmTanh,
        no_state_digest_tensors_v1,
        None,
        2,
    ));
    pins
}

#[test]
fn narrow_dims_refactor_bit_pins_v1() {
    let pins = all_pins_v1();
    let lines = pins.iter().map(PinV1::line_v1).collect::<Vec<_>>();
    for line in &lines {
        println!("{line}");
    }
    let find = |route: &str, update: usize| {
        pins.iter()
            .find(|pin| pin.route == route && pin.update == update)
            .unwrap_or_else(|| panic!("missing pin {route} {update}"))
    };
    for update in 0..UPDATES_V1 {
        // Preparation workers 1, 2 and 4 engage that many forward and
        // exact-backward workers and give one digest.
        for (route, workers) in [
            ("gae_v3_preparation_w1", 1),
            ("gae_v3_preparation_w2", 2),
            ("gae_v3_preparation_w4", 4),
        ] {
            let pin = find(route, update);
            assert_eq!(
                (pin.forward_workers, pin.backward_workers),
                (workers, workers),
                "{route} {update}"
            );
            assert_eq!(
                pin.digest,
                find("gae_v3_preparation_w1", update).digest,
                "{route} {update}"
            );
        }
        // Fixed partitions consult no exact backward, and their worker
        // limit changes no bit.
        for route in ["gae_v3_fixed_partition_w1", "gae_v3_fixed_partition_w4"] {
            let pin = find(route, update);
            assert_eq!((pin.forward_workers, pin.backward_workers), (2, 0));
        }
        assert_eq!(
            find("gae_v3_fixed_partition_w4", update).digest,
            find("gae_v3_fixed_partition_w1", update).digest
        );
        assert_eq!(
            find("terminal_contract_fixed_partition_w4", update).digest,
            find("terminal_contract_fixed_partition_w1", update).digest
        );
        assert_eq!(find("gae_v3_portable_math_w2", update).forward_workers, 2);
        assert_eq!(find("gae_v3_no_state_digest_w2", update).forward_workers, 2);
    }
    let target = EXPECTED_TARGET_V1;
    let expected = EXPECTED_PINS_V1
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();
    assert!(
        !expected.is_empty(),
        "capture mode: no narrow pins recorded for {target}; the {} lines above are this \
         target's pins",
        lines.len()
    );
    assert_eq!(expected.len(), lines.len(), "pin count on {target}");
    for (expected, actual) in expected.iter().zip(&lines) {
        assert_eq!(expected, actual, "narrow pin drifted on {target}");
    }
}
