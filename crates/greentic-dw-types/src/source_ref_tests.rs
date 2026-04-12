#[cfg(test)]
mod tests {
    use crate::{
        PackSourceRef, SourceRef, SourceRefError, SourceRefKind, SourceResolutionPolicy,
        TemplateSourceRef,
    };
    use schemars::schema_for;

    #[test]
    fn source_ref_infers_distributor_kinds() {
        assert_eq!(
            SourceRef::infer_kind("oci://ghcr.io/greenticai/packs/demo:latest").unwrap(),
            SourceRefKind::Oci
        );
        assert_eq!(
            SourceRef::infer_kind("store://greentic-biz/acme/catalogs/foo:latest").unwrap(),
            SourceRefKind::Store
        );
        assert_eq!(
            SourceRef::infer_kind("repo://dw/templates/support-assistant").unwrap(),
            SourceRefKind::Repo
        );
    }

    #[test]
    fn source_ref_supports_local_and_dev_paths() {
        let local = SourceRef::from_raw("./templates/support.json").unwrap();
        assert_eq!(local.kind, SourceRefKind::LocalPath);
        assert!(!local.dev_mode);

        let dev = SourceRef::from_raw("dev://fixtures/template-support.json").unwrap();
        assert_eq!(dev.kind, SourceRefKind::DevPath);
        assert!(dev.dev_mode);
    }

    #[test]
    fn source_ref_rejects_unknown_scheme() {
        let err = SourceRef::from_raw("https://example.com/template.json").unwrap_err();
        assert!(matches!(err, SourceRefError::UnsupportedScheme { .. }));
    }

    #[test]
    fn source_ref_rejects_kind_mismatch() {
        let err = SourceRef::new("repo://dw/templates/support", SourceRefKind::Store).unwrap_err();
        assert!(matches!(err, SourceRefError::KindMismatch { .. }));
    }

    #[test]
    fn wrappers_share_the_canonical_model() {
        let pack = PackSourceRef::from_raw("oci://ghcr.io/greenticai/packs/dw/app:latest").unwrap();
        let template =
            TemplateSourceRef::from_raw("repo://dw/templates/support-assistant").unwrap();

        assert_eq!(pack.source.kind, SourceRefKind::Oci);
        assert_eq!(template.source.kind, SourceRefKind::Repo);
    }

    #[test]
    fn source_ref_schema_is_exportable() {
        let pack_schema = schema_for!(PackSourceRef);
        let pack_schema_text = serde_json::to_value(pack_schema).unwrap().to_string();
        assert!(pack_schema_text.contains("raw_ref"));
        assert!(pack_schema_text.contains("local_path"));

        let policy_schema = schema_for!(SourceResolutionPolicy);
        let policy_schema_text = serde_json::to_value(policy_schema).unwrap().to_string();
        assert!(policy_schema_text.contains("repo_registry_base"));
    }
}
