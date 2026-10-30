use openkf_package::{
    encoded_compact_index_len, read_compact_index, write_compact_index,
    FObjectExport, FObjectImport, MmapPackage, ObjectFlags, PackageWriter,
    UnrealPackage, ENGINE_VERSION_KF, LICENSEE_VERSION_KF, PACKAGE_TAG,
};
use std::path::Path;

const CORE_U_PATH: &str = r"E:\SteamLibrary\steamapps\common\KillingFloor\System\Core.u";
const KFMOD_U_PATH: &str = r"E:\SteamLibrary\steamapps\common\KillingFloor\System\KFMod.u";

#[test]
fn test_compact_index_exhaustive() {
    let test_values = [
        0,
        1,
        -1,
        42,
        -42,
        63,
        -63,
        64,
        -64,
        65,
        -65,
        127,
        -127,
        128,
        -128,
        255,
        -255,
        256,
        -256,
        8191,
        -8191,
        8192,
        -8192,
        16383,
        -16383,
        16384,
        -16384,
        65535,
        -65535,
        65536,
        -65536,
        1_000_000,
        -1_000_000,
        100_000_000,
        -100_000_000,
        i32::MAX,
        -i32::MAX,
    ];

    for &val in &test_values {
        let mut buf = Vec::new();
        write_compact_index(val, &mut buf);
        assert_eq!(
            buf.len(),
            encoded_compact_index_len(val),
            "Length mismatch for value {}",
            val
        );

        let mut offset = 0;
        let decoded = read_compact_index(&buf, &mut offset).expect("Decoding compact index failed");
        assert_eq!(offset, buf.len());
        assert_eq!(decoded, val, "Roundtrip value mismatch for {}", val);
    }
}

#[test]
fn test_parse_core_u() {
    if !Path::new(CORE_U_PATH).exists() {
        eprintln!("Skipping test_parse_core_u: retail file not found at {}", CORE_U_PATH);
        return;
    }

    let pkg = MmapPackage::open(CORE_U_PATH).expect("Failed to open Core.u");

    // 1. Header validation
    assert_eq!(pkg.summary.tag, PACKAGE_TAG, "Invalid magic package tag");
    assert_eq!(pkg.summary.engine_version(), ENGINE_VERSION_KF);
    assert_eq!(pkg.summary.licensee_version(), LICENSEE_VERSION_KF);
    assert_eq!(pkg.summary.name_count, 447);
    assert_eq!(pkg.summary.export_count, 862);
    assert_eq!(pkg.summary.import_count, 18);
    assert_eq!(pkg.summary.name_offset, 0x40);

    // 2. Name table verification
    assert_eq!(pkg.names.len(), 447);
    assert_eq!(pkg.names.get(0), Some("None"));
    assert!(pkg.names.find("Object").is_some());
    assert!(pkg.names.find("Vector").is_some());
    assert!(pkg.names.find("Rotator").is_some());

    // 3. Import table verification
    assert_eq!(pkg.imports.len(), 18);
    let imp0 = pkg.imports.get(0).unwrap();
    assert_eq!(imp0.resolve_object_name(&pkg.names), Some("Core"));
    assert_eq!(imp0.resolve_class_name(&pkg.names), Some("Package"));

    // 4. Export table verification
    assert_eq!(pkg.exports.len(), 862);
    let exp0 = pkg.exports.get(0).unwrap();
    assert_eq!(exp0.resolve_object_name(&pkg.names), Some("Vector"));
    assert_eq!(exp0.serial_size, 24);

    let exp1 = pkg.exports.get(1).unwrap();
    assert_eq!(exp1.resolve_object_name(&pkg.names), Some("Rotator"));
    assert_eq!(exp1.serial_size, 23);

    let exp2 = pkg.exports.get(2).unwrap();
    assert_eq!(exp2.resolve_object_name(&pkg.names), Some("Object"));
    assert_eq!(exp2.serial_size, 81);
    assert!(exp2.is_class_definition());

    // 5. Zero-copy slice verification for all exports
    for (i, export) in pkg.exports.entries.iter().enumerate() {
        let slice = pkg.get_export_data(i).expect("Failed to slice export payload");
        assert_eq!(
            slice.len(),
            export.serial_size.max(0) as usize,
            "Export #{} payload slice size mismatch",
            i
        );
    }

    // 6. Formatted object reference full name
    assert_eq!(pkg.format_full_name(0), "None");
}

#[test]
fn test_parse_kfmod_u() {
    if !Path::new(KFMOD_U_PATH).exists() {
        eprintln!("Skipping test_parse_kfmod_u: retail file not found at {}", KFMOD_U_PATH);
        return;
    }

    let pkg = MmapPackage::open(KFMOD_U_PATH).expect("Failed to open KFMod.u");

    // 1. Header validation
    assert_eq!(pkg.summary.tag, PACKAGE_TAG);
    assert_eq!(pkg.summary.engine_version(), ENGINE_VERSION_KF);
    assert_eq!(pkg.summary.licensee_version(), LICENSEE_VERSION_KF);
    assert_eq!(pkg.summary.name_count, 10889);
    assert_eq!(pkg.summary.export_count, 15506);
    assert_eq!(pkg.summary.import_count, 4151);

    // 2. Name table verification
    assert_eq!(pkg.names.len(), 10889);
    assert_eq!(pkg.names.get(0), Some("None"));
    assert!(pkg.names.find("KFPlayerReplicationInfo").is_some());
    assert!(pkg.names.find("KFWeapon").is_some());
    assert!(pkg.names.find("KFMonster").is_some());

    // 3. Import table verification
    assert_eq!(pkg.imports.len(), 4151);

    // 4. Export table verification
    assert_eq!(pkg.exports.len(), 15506);
    let exp0 = pkg.exports.get(0).unwrap();
    assert_eq!(exp0.resolve_object_name(&pkg.names), Some("KFPlayerReplicationInfo"));
    assert_eq!(exp0.serial_size, 310);

    let exp1 = pkg.exports.get(1).unwrap();
    assert_eq!(exp1.resolve_object_name(&pkg.names), Some("KFWeapon"));
    assert_eq!(exp1.serial_size, 4843);

    let exp2 = pkg.exports.get(2).unwrap();
    assert_eq!(exp2.resolve_object_name(&pkg.names), Some("KFMonster"));
    assert_eq!(exp2.serial_size, 7230);

    // 5. Zero-copy slice verification for all 15,506 exports
    for (i, export) in pkg.exports.entries.iter().enumerate() {
        let slice = pkg.get_export_data(i).expect("Failed to slice export payload");
        assert_eq!(
            slice.len(),
            export.serial_size.max(0) as usize,
            "Export #{} payload slice size mismatch",
            i
        );
    }
}

#[test]
fn test_package_writer_and_reader_roundtrip() {
    let mut writer = PackageWriter::new().with_package_flags(0x0001);

    let _name_none = writer.add_name("None", 0x70010);
    let name_core = writer.add_name("Core", 0x70010);
    let name_class = writer.add_name("Class", 0x70010);
    let name_actor = writer.add_name("Actor", 0x70010);
    let name_weapon = writer.add_name("KFWeapon_Shotgun", 0x70010);

    let imp_pkg = writer.add_import(FObjectImport::new(name_core as i32, name_class as i32, 0, name_actor as i32));

    let payload1 = vec![0xDE, 0xAD, 0xBE, 0xEF, 0x01, 0x02, 0x03, 0x04];
    let exp1 = FObjectExport::new(
        -(imp_pkg as i32) - 1,
        0,
        0,
        name_weapon as i32,
        ObjectFlags::STANDALONE.bits() | ObjectFlags::PUBLIC.bits(),
        payload1.len() as i32,
        0,
    );
    writer.add_export(exp1, payload1.clone());

    let payload2 = vec![0x11, 0x22, 0x33, 0x44];
    let exp2 = FObjectExport::new(
        0,
        0,
        0,
        name_actor as i32,
        ObjectFlags::PUBLIC.bits(),
        payload2.len() as i32,
        0,
    );
    writer.add_export(exp2, payload2.clone());

    let binary = writer.build();

    // Parse back
    let parsed = UnrealPackage::from_bytes(&binary).expect("Failed to deserialize generated package");

    assert_eq!(parsed.summary.tag, PACKAGE_TAG);
    assert_eq!(parsed.summary.engine_version(), ENGINE_VERSION_KF);
    assert_eq!(parsed.summary.licensee_version(), LICENSEE_VERSION_KF);
    assert_eq!(parsed.summary.package_flags, 0x0001);

    assert_eq!(parsed.names.len(), 5);
    assert_eq!(parsed.names.get(name_weapon), Some("KFWeapon_Shotgun"));

    assert_eq!(parsed.imports.len(), 1);
    assert_eq!(parsed.exports.len(), 2);

    let parsed_slice1 = parsed.get_export_data(0).expect("Failed to get payload 1");
    assert_eq!(parsed_slice1, payload1.as_slice());

    let parsed_slice2 = parsed.get_export_data(1).expect("Failed to get payload 2");
    assert_eq!(parsed_slice2, payload2.as_slice());

    assert_eq!(parsed.format_full_name(1), "KFWeapon_Shotgun");
    assert_eq!(parsed.format_full_name(2), "Actor");
}
