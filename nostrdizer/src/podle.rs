use std::str::FromStr;

use crate::errors::Error;
use crate::types::AuthCommitment;

use num_bigint::BigInt;

use bitcoin::consensus::Decodable;
use bitcoin::PrivateKey;
use bitcoin_hashes::{sha256, Hash};
use secp256k1::{PublicKey, Scalar, Secp256k1, SecretKey};

use log::{debug, info};

fn n() -> BigInt {
    "115792089237316195423570985008687907852837564279074904382605163141518161494337"
        .parse()
        .unwrap()
}

fn get_p2(priv_key: SecretKey, nums_key: PublicKey) -> PublicKey {
    let ctx = Secp256k1::new();
    let priv_raw = priv_key.as_ref();
    nums_key
        .mul_tweak(&ctx, &Scalar::from_be_bytes(*priv_raw).unwrap())
        .unwrap()
}

/// Decodes Vec<u8> to BigInt
fn decode(vec: &[u8]) -> BigInt {
    let mut int: BigInt = BigInt::from(0);
    for char in vec {
        int *= 256;
        int += *char;
    }
    int
}

/// Encode a BigInt as a Vec<u8>
fn encode(val: BigInt) -> Vec<u8> {
    let mut result = vec![];
    let mut val = val;
    while val > BigInt::from(0) {
        let cha = &val % BigInt::from(256);
        result.push(cha.try_into().unwrap());
        val /= 256;
    }
    result
}

/// Modulo
fn modulo(a: &BigInt, b: &BigInt) -> BigInt {
    ((a % b) + b) % b
}

/// Generate podle commitment
/// ```
/// use bitcoin::PrivateKey;
/// use nostrdizer::podle::{generate_podle, verify_podle};
///
/// let priv_key = PrivateKey::from_slice( b"\xf00\x1aD3R\xba\xa9&\xce$\xe3\xf6,\xf3j\xden\x87\x85\xee\xe8\xd4c\xd4C\x80\x1f\x81\x02j\xe9", bitcoin::Network::Regtest).unwrap();
/// let result = generate_podle(0, priv_key).unwrap();
///
/// assert_eq!(result.p.serialize(), [2, 30, 229, 220, 10, 194, 200, 105, 195, 110, 225, 178, 244, 49, 52, 230, 190, 215, 102, 72, 155, 101, 23, 157, 93, 141, 120, 51, 3, 66, 151, 108, 172]);
/// assert_eq!(result.p2.serialize(), [3, 244, 231, 197, 180, 185, 249, 244, 106, 38, 41, 229, 149, 221, 9, 249, 222, 147, 89, 33, 173, 206, 237, 228, 134, 107, 138, 213, 252, 51, 51, 243, 147]);
///    // let k =  Scalar::from_be_bytes(*b"\x8d\xe6\xc8-\xc63EYf\xdf\x18\xe7d\xb4\xf9k\xbc\xd6z5\xef\\\xdfvI\xc5\x1b\x07\x87\x91\xcc\x89").unwrap();
/// verify_podle(0, result.clone(), result.commit).unwrap();
/// ```
pub fn generate_podle(index: usize, priv_key: PrivateKey) -> Result<AuthCommitment, Error> {
    let ctx = Secp256k1::new();
    // P
    let pub_key = priv_key.public_key(&ctx).inner;
    //debug!("g P: {:?}", pub_key);
    // k
    let k = Scalar::random();
    // KG
    let kg = SecretKey::from_slice(&k.to_be_bytes())
        .unwrap()
        .public_key(&ctx);

    let j = PublicKey::from_str(PRECOMPUTEDNUMS[index])?;
    // KJ
    let kj = j.mul_tweak(&ctx, &k)?;

    // P2
    let p2 = get_p2(priv_key.inner, j);

    let commitment = sha256::Hash::hash(&p2.serialize());
    // e
    let arrays = [
        kg.serialize(),
        kj.serialize(),
        pub_key.serialize(),
        p2.serialize(),
    ]
    .concat();
    let e = sha256::Hash::hash(&arrays);

    let priv_int = decode(&priv_key.to_bytes());

    let k_int = k.to_be_bytes();
    let k_int = decode(&k_int);

    let e_int = e.into_inner();
    let e_int = decode(e_int.as_ref());

    let sig_int = (k_int + priv_int * e_int) % &n();
    let sig = sig_int.to_bytes_be().1;

    let result = AuthCommitment {
        p: pub_key,
        p2,
        commit: commitment,
        sig,
        e,
    };
    //debug!("Result: {:#?}", result);
    Ok(result)
}

/// Verify a podle commitment
/// ```
/// use nostrdizer::podle::{generate_podle, verify_podle};
/// use bitcoin::PrivateKey;
/// // Not really a great test as it assumes generate is correct
/// let priv_key = PrivateKey::from_slice( b"\xf00\x1aD3R\xba\xa9&\xce$\xe3\xf6,\xf3j\xden\x87\x85\xee\xe8\xd4c\xd4C\x80\x1f\x81\x02j\xe9", bitcoin::Network::Regtest).unwrap();
/// let auth = generate_podle(0, priv_key).unwrap();
///
/// verify_podle(0, auth.clone(), auth.commit);
///
/// ```
pub fn verify_podle(
    index: u8,
    auth_commitment: AuthCommitment,
    fill_commitment: sha256::Hash,
) -> Result<(), Error> {
    // P
    let p = auth_commitment.p;
    let p2 = auth_commitment.p2;
    let sig = auth_commitment.sig;
    let e = auth_commitment.e;
    // TODO: This should be the one previously provided in the FIll
    let commitment = auth_commitment.commit;

    if sha256::Hash::hash(&p2.serialize()) != fill_commitment && commitment != fill_commitment {
        return Err(Error::PodleCommitment);
    }
    debug!("First check passed");

    let sig_priv = secp256k1::SecretKey::from_slice(&sig)?;

    let ctx = Secp256k1::new();
    let s_g = sig_priv.public_key(&ctx);
    let sig_scalar = Scalar::from_be_bytes(sig.try_into().unwrap()).unwrap();

    for i in 0..=index {
        let j = get_nums(i)?;
        debug!("J: {j}");
        let s_j = j.mul_tweak(&ctx, &sig_scalar)?;

        debug!("vP: {}", p);
        debug!("vp2: {}", p2);

        let e_int = <bitcoin_hashes::sha256::Hash as Decodable>::consensus_decode(
            &mut e.into_inner().as_ref(),
        )
        .unwrap();
        let e_int = decode(e_int.as_ref());
        // REVIEW:
        let e_neg = modulo(&-e_int, &n());
        let e_neg = encode(e_neg).try_into().unwrap();

        // REVIEW: Need to be more careful with le vs be bytes
        // would like to be consistent or understand reason for switch
        let e_neg = Scalar::from_le_bytes(e_neg).unwrap();

        let e_p_neg = p.mul_tweak(&ctx, &e_neg)?;
        let e_p2_neg = p2.mul_tweak(&ctx, &e_neg)?;

        let k_g_ser = s_g.combine(&e_p_neg)?;
        let k_j_ser = s_j.combine(&e_p2_neg)?;

        let e_check = bitcoin_hashes::sha256::Hash::hash(
            &[
                k_g_ser.serialize(),
                k_j_ser.serialize(),
                p.serialize(),
                p2.serialize(),
            ]
            .concat(),
        );

        if e_check == e {
            return Ok(());
        }
    }
    Err(Error::PodleVerifyFailed)
}

fn get_g(compressed: bool) -> Vec<u8> {
    let priv_key = [0x00; 31].to_vec();
    let priv_key = [priv_key, vec![0x01]].concat();
    let secret_key = SecretKey::from_slice(&priv_key).unwrap();
    let secp = Secp256k1::new();
    let pub_key = PublicKey::from_secret_key(&secp, &secret_key);
    if compressed {
        pub_key.serialize().to_vec()
    } else {
        pub_key.serialize_uncompressed().to_vec()
    }
}

/// Get nums
fn get_nums(index: u8) -> Result<PublicKey, Error> {
    for &compressed in &[true, false] {
        let mut seed = get_g(compressed);
        seed.extend_from_slice(&[index]);
        for counter in 0..255u8 {
            let mut seed_c = seed.clone();
            seed_c.extend_from_slice(&[counter]);
            let hashed_seed = sha256::Hash::hash(&seed_c);
            let mut claimed_point: Vec<u8> = vec![0x02];
            claimed_point.extend_from_slice(&hashed_seed);

            if let Ok(pubkey) = PublicKey::from_slice(&claimed_point) {
                return Ok(pubkey);
            }
        }
    }
    Err(Error::GetNum)
}

pub const PRECOMPUTEDNUMS: [&str; 256] = [
    "0296f47ec8e6d6a9c3379c2ce983a6752bcfa88d46f2a6ffe0dd12c9ae76d01a1f",
    "023f9976b86d3f1426638da600348d96dc1f1eb0bd5614cc50db9e9a067c0464a2",
    "023745b000f6db094a794d9ee08637d714393cd009f86087438ac3804e929bfe89",
    "023346660dcb1f8d56e44d23f93c3ad79761cdd5f4972a638e9e15517832f6a165",
    "02ec91c86964dcbb077c8193156f3cfa91476d5adfcfcf64913a4b082c75d5bca7",
    "02bbc5c4393395a38446e2bd4d638b7bfd864afb5ffaf4bed4caf797df0e657434",
    "02967efd39dc59e6f060bf3bd0080e8ecf4a22b9d1754924572b3e51ce2cde2096",
    "02cfce8a7f9b8a1735c4d827cd84e3f2a444de1d1f7ed419d23c88d72de341357f",
    "0206d6d6b1d88936bb6013ae835716f554d864954ea336e3e0141fefb2175b82f9",
    "021b739f21b981c2dcbaf9af4d89223a282939a92aee079e94a46c273759e5b42e",
    "025d72106845e03c3747f1416e539c5aa0712d858e7762807fdc4f3757fd980631",
    "02e7d4defb5d287734a0f96c2b390aa14f5f38e80c5a5e592e4ce10d55a5f5246b",
    "023c1bf301bcfa0f097f1a3931c68b4fd39b77a28cc7b61b2b1e0b7ca6d332493c",
    "0283ac2cdd6b362c90665802c264ee8e6342318070943717faee62ef9addeff3e9",
    "02cb9f6164cd2acdf071caef9deab870fc3d390a09b37ba7af8e91139b817ce807",
    "02f0a3a3e22c5b04b6fe97430d68f33861c3e9be412220dc2a24485ea5d55d94db",
    "02860ca3475757d90d999e6553e62c07fce5a6598d060cceeead08c8689b928095",
    "0246c8eabc38ce6a93868369d5900d84f36b2407eecb81286a25eb22684355b41d",
    "026aa6379d74e6cd6c721aef82a34341d1d15f0c96600566ad3fa8e9c43cbb5505",
    "02fdeacb3b4d15e0aae1a1d257b4861bcc9addb5dc3780a13eb982eb656f73d741",
    "021a83ecfaeb2c057f66a6b0d4a42bff3fe5fda11fe2eea9734f45f255444cddc0",
    "02d93580f3e0c2ec8ea461492415cc6a4be00c50969e2c32a2135e7d04f112309a",
    "0292c57be6c3e6ba8b44cf5e619529cf75e9c6b795ddecd383fb78f9059812cb3f",
    "02480f099771d0034d657f6b00cd17c7315b033b19bed9ca95897bc8189928dd47",
    "02ac0701cdc6f96c63752c01dc8400eab19431dfa15f85a7314b1e9a3df69a4a66",
    "026a304ceb69e37d655c1ef100d7ad23192867151983ab0d168af96afe7f1997f6",
    "023b9ff8e4a853b29ecae1e8312fae53863e86b8f8cb3155f31f7325ffb2baf02c",
    "021894ce66d61c33e439f38a36d92c0e45bf28dbc7e30bfb4d7135b87fc8e890e1",
    "02d9e7680e583cf904774d4c19f36cb3d238b6c770e1e7db03f444dc8b15b29687",
    "024350c7ff5b2bf2c58e3b17a792716d0e76cff7ad537375d1abc6e249466b25a3",
    "02c6577e1cdcbcfadb0ae037d01fbf6d74786eecdb9d1ee277d9ba69b969728cfe",
    "029f395b4c7b20bcb6120b57bee6d2f7353cd0aa9fe246176064068c1bd9b714d1",
    "02d180786087720b827bf04ae800547102470a1e43de314203e90228c586b481a1",
    "023548173a673965c18d994028bc6d5f5df1f60dccf9368b0eae34f8cff3106943",
    "02118124c53b86fdade932c4304ad347a19ce0af79a9ab885d7d3a6358a396e360",
    "02930bcdee5887fa5a258335d6948017e6d7f2665b32dcc76a84d5ca7cd604d89b",
    "0267e79a47058758a8ee240afd941e0ae8b4f175f29a3cf195ad6ff0e6d02955b1",
    "027e53d9fb04f1bb69324245306d26aa60172fd13d8fe27809b093222226914de6",
    "02ef09fbdcd22e1be4f0d4b2d13a141051b18009d7001f6828c6a40b145c9df23e",
    "028742fd08c60ba13e78913581db19af2f708c7ec53364589f6cbcf9d1c8b5105f",
    "020ce14308d2f516bf4f9e0944fb104907adef8f4c319bfcc3afab73e874a9ce4a",
    "027635f125f05a2548201f74c4bbdcbe89561204117bd8b82dfae29c85a576a58e",
    "02fe878f3ae59747ee8e9c34876b86851d5396124e1411f86fe5c58f08f413a549",
    "02f2a6af33bd08ab41a010d785694e9682fa1cc65733f30a53c40541d1c1bfb660",
    "02cbe9d18b6d5fc9993ef862892e5b2b1ea5d2710a4f208672c0f7c36a08bb5686",
    "023fb079b25c0a8241465fb55802f22ebb354e6da81f7dabfe214ddbd9d3dfcd5a",
    "021a5b234b9a10fc5f08ed9c1a136a250e92156adc12109a97dd7467276d6848a8",
    "0240fbe9363d50585da40aef95f311fc2795550e787f62421cd9b6e2f719bb9547",
    "02a245fbbc00f1d6feb72a9e1d3fd0033522839d33440aea64f52e8bccee616be8",
    "02fd1e94bb23a4306de64064841165e3db497ae5b246dabff738eb3e6ea51685a7",
    "0298362705914c839e45505369e54faefbb3aaebb4c486b4d6e59ca03304f3552c",
    "021b8109a23b858114d287273620dd920029d84b90f63af273c1c78492b1a70105",
    "028df6ce4fec30229cddb86c62606cff80e95cb8277028277f3dcc8ac9f98eef9d",
    "02ed02925d806df4ac764769d11743093708808157fb2933eb19af5399dcfd500c",
    "02ce88da0e81988bd8f5d63ad06898a355f7dc7f46bb08cf5f1e9bc5c3752ad13c",
    "02f4868cc8285cd8d74d4213d18d53d5f410d50223818f1be6fe8090904e03743d",
    "02770cecdf18aa2115b6e5c4295468f2e92a53068dc4295d0e5d0890b71d1a2fcc",
    "02b5d4dce8932de37c6ef13a7f063f164dfd07f7399e8e815e22b5af420608fd2a",
    "0284ad07924dbac50a72455aec3ddba50b1ed71e678ba935bb5d95c8a8232b1353",
    "02cb8c916a6f9bc39c8825f5b0378bb1b0a0679e191843aa4db2195b81f14c87e0",
    "0235aa30ec3df8dd193a132dbaf3b351af879c59504ed8b7b5ad5f1f1ea712854f",
    "02df91206e955cefe7bcda4555fc6ad761b0e98d464629f098d4483306851704e9",
    "02ed4f1fccd47e66a8d74e58b4f6e31b5172b628fc0dacdb408128c914eb80f506",
    "0263991bb62aaca78a128917f5c4e15183f98aefddf04070c5ca537186f1c1a97a",
    "02ffe2b017882d57db27864446ad7b21d3855ae64bddf74d46e3a611bf903580be",
    "02d647aba2c01eecd0fac7e82580dd8b92d66db7341d1b65a5e4b01234f1fbb2cd",
    "023134ff85401dba9aff426d3f3ba292ea59684b8c48ea0b495660797a839246a6",
    "02827880fe0410c9ea84f75a629f8f8e6eed1f26528af421cf23b8ecf93b6b4b7b",
    "02859b3f9f1f5ba6aa0787f8d3f3f2f21b4932c40bc36b6669383e3bbd19654a5f",
    "02a7d204dfc3eed44abd0419202e280f6772efd5acf9fd34331b8f47c81c6dab19",
    "02e15d11b443a9340ac31a8c5774ce34cd347834470c8d68c959828fae3a7eb0c6",
    "029931f65e46627d60519bfd08bd8a1bb3d8d2921f7f8c9ef31f4bfcdd8028ead2",
    "02e5415ba78743d736018f19757ee0e1ca5d4a4fb1d0464cd3eea8d89b34dd37b8",
    "027ea7860afc3de502d056d9a19ca330f16cd61cfefbeb768df68a882d1f8f15f5",
    "026c19becac43626582622e2b7e86ebd8056f40aa8ab031e70f4deae8cab34503f",
    "02098dab044c888ddebe6713fcb8481f178e3ba42d63310b08d8234e20fe1de13f",
    "02ed6af1a2bebcb381ce92f87638267b1afefe7a1cdce16253f5bf9f99a84ce4b2",
    "023d8493f9e72cd3212166de50940d980f603ae429309abb29e15cccc1983efe37",
    "025c07d7513b1bae52a0089a4faee127148e2ba5a651983083aedc1ae8403cf1eb",
    "0285a93a8c8e6134b3a53c5bd1b5b7d24e7911763ea887847c5d66af172ed17f10",
    "02fea28fb142aa95fcd44398c9482a3c185ec22fee8f24ad6b2297ac7478423f21",
    "02f9840a1635ae3fa131405526974d40d2edee17adf58278956373ce6c69757c2a",
    "023579e441a7dcdbd36a2932c64fa3318023b1f3d04daab148622b7646246a6d7c",
    "02bcbc2933f90a88996c1363c8d3a7004e0c6b75040041201fb022e45acb0af6a7",
    "02cd52e0d28f5564fc2bf842fa63dfefbcf2bb5fe0325703c132be5cd14cca7291",
    "021e648e261b93fedd3439352899c0fa1acedd1f68ab508050a13ed3cbbc93c2ff",
    "0295f9caea5f57d11b12ddee154a36a14921a8980fa76726e48e1d76443d4e306f",
    "02396edf4c18283dd3ef68a2c57b642bd87ae9f8b6be5e5fe4a41c5b86c5db8eb2",
    "0264f323ca3eee79385c9bfd35cd4cf576e51722f38dd98531d531a29913e5170d",
    "02facd3f63f543e0ab9b13323340113acbe8ed3bafdfabdc80626cdd15386c80f3",
    "02b6762640f96367fbf65eecfafcee5c6f7d6a42b706113053bb36a882659d3e65",
    "02ed63f2eca15d9b338fcdb9b3efa3b326e173a1390706258829680f7973fa851c",
    "026f6d47d0d48ff13d64ec6a1db2dc51173cee86ab8010a17809b3fe01483d9fc5",
    "02814e7cae580a1ef86d6ee9b2f9f26fe771e8ea47acf11153b04680ada9cd3042",
    "020e46225fb3ee8f04d08ffbe12d9092ff7f7227f9cb55709890c669e8a1c97963",
    "028194469e8d6ee660e95d6125ba0152ad5c24bf7e452adf80db7062d6926851c4",
    "02b3e1f5754562635ebeecfd32edb0d84a79b2f0c270bac153e60dd29334dc2663",
    "02afff20730724a2d422f330e962362e7831753545ac0a931dd94be011ccf93e9c",
    "02a9cfdf0471a34babfc2f6201dbc79530f3f319204daedb7ec05effc2bdfc5a74",
    "02838fe450f2dd0c460b5fae90ec2feb5b7f001f9cd14c01a475c492cf16ea594b",
    "02aacc3145d04972d0527c4458629d328219feda92bef6ef6025878e3a252e105a",
    "02720fe09616d4325d3c4c702a0aeafbbbff95ef962af531c5ae9461ec81fdf8c5",
    "02e6408f24461a6c484f6c4493c992d303211d5e4297d34afede719a2b70c96c14",
    "02b9ecf2d3fdf2611c6d4be441a0f9a3810dadae39feb3c0d855748cc2dd98a968",
    "027a32d12a536af038631890a9b90ee20b219c9c8231a95b1cde24c143d8173fec",
    "02d26c98fb50b57b7defdf1e8062a52b2a859ba42f3d1760ee8ff99c4e9eb3ec03",
    "02df85556e8d1e97a8093e4d9950905ebced0ea9a1e49728713df1974eeb455774",
    "021fe1dbada397155a80225b59b4fb9a32450a991b2d9d11d8500e98344927c856",
    "0211ccd0980a9ab6f4bb82fdc2e2d1ddace063a7bc1914a6ab4d02b0fa1ca746ec",
    "0264bd41f41aad19f8bfd290fd3af346ebbf80efd33f515854f82bd57e9740f7aa",
    "0226d5fb607cadb8720e900ce9afb9607386ad7b767e4ab3a4e0966223324b92eb",
    "02b3bbf2e2ceae25701bd3b78ba13bea3f0dfed7581b8a8a67c66de9fd96ee41e2",
    "024b8dd765e385d0e04772f3dbf1b1a82abc2de3e5740baac1f6306cd9fd45fe99",
    "022153f6a884ae893ebb0642a84d624c0b62894d7cb9e2a48a3a0c4696e593f9db",
    "0245e22b6388cb14c9c8dbcac94853bdf1e81816c07e926a82b96fc958aa874626",
    "02cba97826b089c695b1acffdcdbf1484beec5eb95853fea1535d6d7bdb4e678b0",
    "02ed006fbab2d18adbd96d2f1de6b83948e2a47acc8d2f92d7af9ba01ffae58276",
    "02513592f4434ee62802d3965f847684693000830107c72cd8de4b34e05b532dae",
    "028adc75647453a247bd44855abb56b60794aaed5ce21c9898e62adac7adcfbe8e",
    "02a712d5dc572086359f1688e8e7b9a5f7fc3079644aea27cdddb382208fee885b",
    "029abf8551218c9076f6d344baa099041fe73e5e844aac6e5c20240834105cdf60",
    "027d480071a2d128c51e84c380467e1ac8435f05b985bbfee0099d35b4121fb0ca",
    "02a7f2e4253fa0d833beca742e210c0d59a4ffc8559764766dcffb1aa3e4961826",
    "023521309a6bdfafdf7bdae574a5f6010eb992e4bae46d8f83c478eac137889270",
    "02b99fe8623aa19ca2bed6fe435ae95c5072a40193913bebe5466f675c92a31db7",
    "02dc035112a2b4881917ea1db159e7f35ee9d98d31533e1285ca150ce84e538e4f",
    "0291a07ecce8061561624de7348135b9081c5edd61541b24fa002fb6c074318fec",
    "020d8a5253d7e0166aa37680a5f64cab0cdad2cdc4c0e8ae61d310df4c4f7386eb",
    "026285db47fee60b5ad54cbd4c27a4e0cd723b86a920f03b12dc9b8c5f19f06448",
    "020f94a9df4302f701b4629f74d401484daf84c7aabaf533f8c21c1626009e923c",
    "027bb78af54b01ddad4e96b51a4e024105b373aab7e1a6ec16279967fcbbb096b4",
    "02e1b20c0da3b8c991f8909fd0d31874be00e9fcb130d7c28b8ad53326cdf13755",
    "02bbdd4dfc047f216e2cbff789bcf850423bedf2006d959963f75621810fecf0d9",
    "024e1fe4b23feda8651a467090e0ce7e8b8db2ccb1c27d52255c76754aa1940d1b",
    "0241aad8f575556c49c4fefae178c2c38541962bfff2ca84ebecea9f661ccf3536",
    "02bcf6203d725ca0640bd045389e854e00087c54ba01fd739c6ef685b22f89340c",
    "0202178e6b3a9b498399aa392b32dc9010f1eea322a6d439ad0c8cacf2008b3e34",
    "026db3289d470df0fdf04f5f608fae2d7ec4ddbd3de2603f6685789520bdee01fc",
    "0239bcfc796488129e3b2f01e6fbbda2f1b357b602e94b5091b44c916e9806dc34",
    "020513bc4a618d32d784083f13d46e6c6d547f01b24942351760f6dc42e2bb7167",
    "0204d2495e4fc20e0571ab2fcb4c1989fdda4542923aa97fe1a77a11c79ade1964",
    "021eaa6af99ea4f1143a45a1b5af7b2d3c3e8810f358be6261248c5ba2492a7b4e",
    "02799849e87e3862170add5b28a3b7e54b04cc60c2cec39de7eca9bfdfaaf930a8",
    "02639bced287084268136c5b6e9e22f743b6c8f813e6aabe39521715bfa4a46ab8",
    "0283c8b21fc038c1fbeedfae0b3abc4dbde672b0dcfda540f9fcfcf8c6e6d29fc3",
    "02b284f4510535ff98e683f25c08b7ae7dd19f7b861e70a202469ddfb2877bc729",
    "0256af1c82cde40ffd03564368b8256a5e48ef056df2655013f0b1aa15de1de8d2",
    "02964b55eab2f19518ee735cae2f7f780bfab480bcbd360f7a90a2904301203366",
    "02f046486f4a473f2226f6bd120aafc55a5c8651f3eb0855aa6a821f69f3016cc6",
    "02eb8dfb7c59fbf24671e258ca5e8eda3ea74c5f0455eed4987cfda79f4fcf823f",
    "020fac2c37cc273d982c07b2719a3694348629d5bdaebc22967fb9d0e1d7f01842",
    "025c0c8ff9a102f99f700081526d2b93b9d51caf81dcf4d02e93cf83b4a7ff5c92",
    "02a118f5fa9c5ef02707e021f9cb8056e69018ef145bec80ead4e09c06a60050c1",
    "029ea72333d1908bb082bffec9da8824883df76a89709ab090df86c45be4abf784",
    "02bacc52256e5221dbfc9a3f22e30fa8e86ddd38e3877e3dc41de91bdcf989b00b",
    "02bc8b37dc66e2296ae706c896f5b86bd335f724cfa9783e41b9dc5e901b42b1de",
    "02eca1099cea9bcab80820d6b64aec16dce1efa0e997b589f6dba3a8fd391fb100",
    "027f1c1bb99bd1a0e486f415f8960d45614a6fcac8cedc260e07197733844827d0",
    "021fc54df458bcfafc8a83d4759224c49c4b338cf23cd9825d6e9cdeffc276375b",
    "027d4fff88da831999ba9b21e19baf747dc26ea76651146e463d4f3e51c586ee91",
    "02e49c0fef0ebc52908cdcea4d913a42e5f24439fffdfaa21cc55a6add0ad9d122",
    "0208b5e8e5035fdb62517d4ebab0696775dbfbdba8ff80f2031c1156cda195a2ab",
    "0202e990bab267fff1575d6acc76fe2041f4196f4b17678872f9c160d930e5be35",
    "02c73fcedd9f6eabc8fe4e1e7211cdb0f28967391d200147d46e4077d2915c262d",
    "0261490abc5f14387ef585f42d99dbddb0837b166694d4af521086a1ffd46e5640",
    "02b46a143e4e0af20a12c39f3105aca57ca79f9332df67619ee859b5d9bffb6d6d",
    "0299f53c064d068f003f8871acae31b84ddda9d8dbe516d02dc170c70314ee2af7",
    "023305144dccba65c67001474ee1135aa96432f386b5eb27582393b2ed4bfc185d",
    "02e044b70ff7e9c784b3c40d09bdfadd4a037e692b0b3aa9ab6bb91203f86a0b37",
    "02ded067a2e44282b0d731a28ffbd03ca6046c5b1a262887ea7cab4810050fbb8c",
    "02e00e4c9198194d92a93059bce61f8249e1006eee287aa94fe51bb207462e5492",
    "0241b89d9164f4c07595ca99b7d73cad2b20ac39847cf703dff1d7d6add339ebeb",
    "02eba24cd4946e149025a9bf7759df5362245bf7c53c5a3205be0c92c59db8d5dc",
    "026bd40c611246a789521c46d758a80337ff40bb298a964612b2af74039211727a",
    "02b9095e071e4edfddf8afb0e176536957509d23f90fb7175ad086b4098e731c73",
    "0214ad0014dfddc5c7eb0801b97268c1b7e03d64215d6b9d5ed80b468089e4a01d",
    "02c455b8e38103ade8794fb51a1656e1439b42bdf79afd17a9df8542153914a7cf",
    "02cc89d6437fdcf711a76eb16f4014f2e21b71740afc8b3ec13ccb60a45b12d815",
    "0208eee5857dda0ae1c721e6ed4c74044add4e1ce66f105413e9ef1cccbdca87ad",
    "02edc663693827cad44d004ac24753bfc3167f81ff4074bb862453376593229c0f",
    "0202a4b7fb31e30b6d8f90a5442ef31f800902ea7a9511e24437b7a0ef516f79a9",
    "02ff05472c2019ac2c9ab8b7fcb0604a94b7379c350306be262144588ea252d0f4",
    "02b131bb594a1270d231e18459e484c49f3eca3b3b2291c9be81c01dc8a4037fa1",
    "02f50125277ea19f633e93868cf8e8a4cd76b21eedf8e3ef59de43f40d73a01d01",
    "027aab228a7d6f87003b01fb9c0b9bcfb2098adbc76f5f9b856aedd28077fc4471",
    "02925200e4f74bea719a99f4a0b05165b9af475f2187381bd0b79cad4d5f2593b6",
    "02c311f1750c6d5c364b71c3b0f369f6959d34a3718da695c5b227ecf1a4669bf6",
    "02cb030c71169d0a1ae30ffba92311bc06bb64b27570598dedabdea0b24631a0ca",
    "02e64669898eecff7aa887307be696a694f61559e7ca41119677b7e94f37cd2914",
    "028fe93e32c24df7f8aaf8d777335fd9ce9f9b5c121dec2ab1ff21575c047497e7",
    "026f08c1c3cb4cff5cdbd7985db4a8ebf0ebc0924530b0fa118d095c4667efeb52",
    "02afe08dbba6c999efb73aeae1da0ad8b143a1b51759caffd3ed2de4494adc47fb",
    "02e99aec0b5e869b3885a3b9f527fd3c546dde83d41a5a156703d0da5e10e04743",
    "02b7e5f4cb9233107bf7a47789dca4eb811af108822f2d4bd03dec13251ec45984",
    "023b971e135daa0b851797b17e3a1cc5ac8a9a6207a2e784a0fe36732a00407b49",
    "02b1742739bfbb528b2a2731cb5d5f1bd03f4fa9c94607837e586c7c6f6589be4a",
    "022cd1b023bb2afc68ee27b40f8deb1d1c6d7b7aa97c32c444f1ceebd449dbeb22",
    "02704e21f8bf38158d7e8100e297adfc930c14c8791beee9b907407f4ca654d95b",
    "02caabeb678374ca75bd815c370b2e37fb0470591557219d6289b1b1e655ed80c6",
    "026aa8d45112aa0da335054194c739e04787526250493f5a0eaaa8a346541d1a0f",
    "022fb12408355439bbee33066bbeefcffb0bdc9cfd1950510fd2a42bdc4eaa1d53",
    "02639fe47769f7694ca6dbfd934762472391d70b23868a58e11d2bd46373e1df29",
    "02f75360f52df674247c5f005b3451ee47becf3204862154d4e7ee97a0e40df3d2",
    "0230241e27d0d3ad727d26472541fcd48f2bb128db5611237fa9f33f86ede8d5c9",
    "0255d5a0aa37a226c001f6b7f19e2bddb10aeaa0652430b8defe35c3f03dfb3c0e",
    "024e6faa398b0acf8a8dfdd9d21e0a46a22d07cd0fcffd89749f74f94f9993f4d9",
    "020c1a256587306f58f274cc2238f651bbfadfd42436e6eb8f318ac08fae04e7ae",
    "025858b8188da173e8b01b8713b154ffae8b2d2eb8f9670362877102cf0c0c4f28",
    "02dc7509c77d7fa61c08c5525fb151bf4fe12deb1989a3be560a63105dae2ecd2e",
    "02a272df6dab1c22c209b45b601737c0077acb7869bb9fe264c991b4ef199e337d",
    "025168f2fdd730b4c33b57d3956e6a40dd27a4f32db70d9f9b5898fa2bed3de342",
    "028133baac70bc2c2ebe8a22af04b5faedd070e276c90e2f910bb9bf89441a80db",
    "029064628ebd6e97a945c1d52641a27bff3c4f59659e657b88d23c2ce1c4d04644",
    "023cf20c4e8675bce999a0128602fe21699db651540f3dcbe7a4ef2126243ba17a",
    "02cc685739a4b20e2d52ddf256e597c06b7eb69e65d009820c6744b739c7215340",
    "02d061544ce21398af3e0e6c329ce49976a9ecd804ebc543f4c16f6a32798f37c2",
    "029fe49ff440f23c69360a92d249db429bdc3601fc8a5a3fc1aa894de817c05490",
    "0222c8c4e90585f9816b5801bad43fb608857269fdaaefbe2b5b85903231685679",
    "0296b72ed4968860b733fb99846698df2e95c65af281b3ef8b5ab90e2d5de966cb",
    "02c27565a7fd5d1f4bcbe969bddbace99553fb65cb7750965350ff230b1f09f97d",
    "02e1254be9833236609bf44c62ef6da7188a44bbe2d53a72cf39a38ef9f99bb783",
    "0280663ce16afadc77e00ade780da53e7c11b02a66cbf36837ef7d9d2488f23417",
    "02ad8b11e62c6753917307bdde89a42896e0070d33f6f93c608d82f6d041b814a4",
    "02ce1d943dfc14654266507def2b7b9940bffceb4f54d709a149f99962083398fc",
    "023ea7eb26248c05beb4e4d8ba9f9785d5fd1a55d3137c90f40b807b60aa4262df",
    "0211c802fec9b31710d3849e2c1700cea5374ae422e54551946d96fc240c63fba0",
    "02204ad97ebe2ec30d6db1bfc1e1d4660331909668634c3cd928b5c369a6013367",
    "020251bf4271d359a082cdad23d9a5cd48916d78eed010fe1e7d9711cd420b3cdf",
    "0292b9757195350676e447e49425f887d3df7e27774bb3e0aab5b528da0a1a0340",
    "022be18362b2a167199a76f6065358063b1167d5bbcfe7652fc55f93a5ebd42e89",
    "02e6b1e618efe5f468bdb40f5ec167ed4fa7636849c4ff4ddab0199c903b37306c",
    "02a6676873de91890ecae000c575e46e4a9629865fb1662606da5e9c1fdcd55d5c",
    "02c088a3c96b13413caa5f32a8f4640e76ec0a37990577d679d2062e859547f058",
    "023e9703ed6209d5a25e0ecb34e04c22f274f37845aa2a4e2f2343e39928360e25",
    "02977d845787c4690152827bfd15e801044c84d33430a7ed928499e828cf131d14",
    "0224ea648555445d1305aaf6bd74fda3041b2a10bf7900a4c067462b01c6dc25f1",
    "02dfd472c98ece1dc2a18c1bebf98a09990fba673e725c029928937247022b9d24",
    "02a2a03933d06617adcf0f4ad692e95d463a5fa9938e8d451e5d6271f4a5af8bb4",
    "02ca24fa8d7aa53f7f5b4e1ca16eb6fd9b9cfb0162a332abb7a88ddf8e964c99bc",
    "02bbce92d1db3ef0c9c09793b760fd3b929c9168e4dff396c618fa0ed3cf6a5edb",
    "028af15d26d3b297f4d2aeaf308632b60251accf87aa8470b3d4d1ef2dabb99209",
    "021b81c0e878389231339fd9d622a736fc9d36de93a58ea6a4bc38fef86672278a",
    "021adc24309f605c7a5af106e8b930feaec0bec6545fb4c70b83ebe5cf341cab2d",
    "020462a3ff101ac379f87f43190459b7494f4128ea30035877ce22a35afb995e34",
    "02f1019851779a6d0db09e8abeba3b9a07b6931b43b0d973cfe261a96b4516cca4",
    "02d7023276f01ff22a9efeadd5b539d1d9ceb80ebf6813e6042a49c946a82f366f",
    "021594f45af3a21e0210a2ca4cbc3e95ea95db5aca3561fc1f759cb7f104dd0f62",
    "021398309b6c293c0dc28cdd7e55ad06306b59cb9c10d947df565e4a90f095a62a",
    "029f39d84383200e841187c5b0564e3b01a2ba019b86221c0c1dd3eae1b4dabb26",
    "0252ec719852f71c2d58886dd6ace6461a64677a368b7b8e220da005ac977abdc8",
    "0237f0d7de84b2cc6d2109b7241c3d49479066a09d1412c7a4734192715b021e06",
    "021e9e0e4784d15a29721c9a33fbcfb0af305d559c98a38dcf0ce647edd2c50caa",
    "02e705994a78f7942726209947d62d64edd062acfa8a708c21ac65de71e7ae71df",
    "0295f1cafd97e026341af3670ef750de4c44c82e6882f65908ec167d93d7056806",
    "023a0d381598e185bbff88494dc54e0a083d3b9ce9c8c4b86b5a4c9d5f949b1828",
    "02a0a8694820c794852110e5939a2c03f8482f81ed57396042c6b34557f6eb430a",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_modulo() {
        let a = "-22194981318972513906404150772491931704704772619352044137778275718648945750476"
            .parse()
            .unwrap();
        let b = n();

        let m = modulo(&a, &b);

        assert_eq!(
            m,
            "93597107918343681517166834236195976148132791659722860244826887422869215743861"
                .parse()
                .unwrap()
        );
    }

    #[test]
    fn test_decode() {
        let v = [
            141, 230, 200, 45, 198, 51, 69, 89, 102, 223, 24, 231, 100, 180, 249, 107, 188, 214,
            122, 53, 239, 92, 223, 118, 73, 197, 27, 7, 135, 145, 204, 137,
        ];

        let result = decode(&v);

        let r: BigInt =
            "64183868058479472664368820583086059908285866182535387296062357430386065263753"
                .parse()
                .unwrap();

        assert_eq!(result, r);
    }

    #[test]
    fn test_encode() {
        let v = "98904036365135577215743764907591587298480678091079165657051126266420213344278"
            .parse()
            .unwrap();

        let e = encode(v);

        let result = vec![
            22, 104, 108, 178, 243, 102, 30, 118, 72, 225, 0, 161, 104, 24, 97, 0, 231, 164, 103,
            57, 134, 125, 113, 118, 202, 79, 60, 34, 104, 179, 169, 218,
        ];

        assert_eq!(e, result);
    }

    #[test]
    fn test_get_nums() {
        let mut nums = vec![];
        for i in 0..=255 {
            nums.push(get_nums(i).unwrap().to_string());
        }
        assert_eq!(nums.as_slice(), PRECOMPUTEDNUMS)
    }

    #[test]
    fn test_get_p2() {
        let priv_key = PrivateKey::from_slice( b"\xf00\x1aD3R\xba\xa9&\xce$\xe3\xf6,\xf3j\xden\x87\x85\xee\xe8\xd4c\xd4C\x80\x1f\x81\x02j\xe9", bitcoin::Network::Regtest).unwrap();
        let j = PublicKey::from_str(PRECOMPUTEDNUMS[0]).unwrap();
        let p2 = get_p2(priv_key.inner, j);

        assert_eq!(
            p2.to_string(),
            "03f4e7c5b4b9f9f46a2629e595dd09f9de935921adceede4866b8ad5fc3333f393".to_string()
        );
    }
}
