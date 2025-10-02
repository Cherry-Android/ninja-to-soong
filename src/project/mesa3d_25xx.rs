// Copyright 2025 ninja-to-soong authors
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[derive(Default)]
pub struct Mesa3DPanVK {
    src_path: PathBuf,
}

const DEFAULTS: &str = "mesa3d-25xx-defaults";
//const VIR_DEFAULTS: &str = "mesa_platform_virtgpu_defaults";

impl Project for Mesa3DPanVK {
    fn get_name(&self) -> &'static str {
        "mesa3d-25xx"
    }
    fn get_android_path(&self) -> Result<PathBuf, String> {
        Ok(Path::new("external").join(self.get_name()))
    }
    fn get_test_path(&self, ctx: &Context) -> Result<PathBuf, String> {
        Ok(ctx.test_path.join(self.get_name()))
    }
    fn generate_package(
        &mut self,
        ctx: &Context,
        _projects_map: &ProjectsMap,
    ) -> Result<String, String> {
        self.src_path = ctx.get_android_path(self)?;
        let ndk_path = get_ndk_path(&ctx.temp_path, ctx)?;
        let build_path = ctx.temp_path.join(self.get_name());

        let mesa_clc_path = if !ctx.skip_build {
            let mesa_clc_build_path = ctx.temp_path.join("mesa_clc");
            execute_cmd!(
                "bash",
                [
                    &path_to_string(self.get_test_path(ctx)?.join("build_mesa_clc.sh")),
                    &path_to_string(&self.src_path),
                    &path_to_string(&mesa_clc_build_path),
                ]
            )?;
            mesa_clc_build_path.join("bin")
        } else {
             PathBuf::new()
        };

        if !ctx.skip_gen_ninja {
            execute_cmd!(
                "bash",
                [
                    &path_to_string(self.get_test_path(ctx)?.join("gen-ninja.sh")),
                    &path_to_string(&self.src_path),
                    &path_to_string(&build_path),
                    &path_to_string(&ndk_path),
                    &path_to_string(mesa_clc_path),
                ]
            )?;
        }
        common::ninja_build(&build_path, &Vec::new(), ctx)?;

       // Clean libdrm to prevent Soong from parsing blueprints that came with it
        if !ctx.skip_gen_ninja {
            execute_cmd!(
                "git",
                [
                    "-C",
                    &path_to_string(&self.src_path),
                    "clean",
                    "-xfd",
                    "subprojects/libdrm*"
                ]
            )?;
        }

        const MESON_GENERATED: &str = "meson_generated";
        let mut package = SoongPackage::new(
            &["//visibility:public"],
            "mesa3d-25xx_licenses",
            &[
                "SPDX-license-identifier-MIT",
                "SPDX-license-identifier-Apache-2.0",
                "SPDX-license-identifier-GPL-1.0-or-later",
                "SPDX-license-identifier-GPL-2.0-only",
            ],
            &[
                "licenses/MIT",
                "licenses/Apache-2.0",
                "licenses/GPL-1.0-or-later",
                "licenses/GPL-2.0-only",
            ],
        )
        .generate(
            NinjaTargetsToGenMap::from(&[
                target!("src/egl/libEGL_mesa.so", "libEGL_mesa"),
                target!("src/mapi/es1api/libGLESv1_CM_mesa.so", "libGLESv1_CM_mesa"),
                target!("src/mapi/es2api/libGLESv2_mesa.so", "libGLESv2_mesa"),
                target!("src/gbm/backends/dri/dri_gbm.so", "dri_gbm"),
                target!(
                    "src/gallium/targets/dri/libgallium_dri.so",
                    "libgallium_dri"
                ),
                target!("src/gbm/libgbm_mesa.so", "libgbm_mesa"),
                target!(
                    "src/panfrost/vulkan/libvulkan_panfrost.so",
                    "vulkan.panfrost"
                ),
            ]),
            parse_build_ninja::<MesonNinjaTarget>(&build_path)?,
            &self.src_path,
            &ndk_path,
            &build_path,
            Some(MESON_GENERATED),
            self,
            ctx,
        )?;

        let gen_deps = package
            .get_gen_deps()
            .into_iter()
            .filter(|include| !include.starts_with("subprojects"))
            .collect();
        package.filter_local_include_dirs(MESON_GENERATED, &gen_deps)?;
        common::clean_gen_deps(&gen_deps, &build_path, ctx)?;
        common::copy_gen_deps(gen_deps, MESON_GENERATED, &build_path, ctx, self)?;

        // HACK: remove one cflag from dri_gbm to have common defaults
        let prop_cflags =
            SoongNamedProp::get_prop(&package.get_props("dri_gbm", vec!["cflags"])?[0]);
        let mut cflags = match prop_cflags {
            SoongProp::VecStr(t) => t,
            _ => Vec::new(),
        };
        cflags.retain(|a| a != "-pthread");

        let default_module = SoongModule::new("cc_defaults")
            .add_prop("name", SoongProp::Str(String::from(DEFAULTS)))
            .add_prop("cflags", SoongProp::VecStr(cflags));

        package.add_module(default_module).print(ctx)
    }

    fn extend_module(&self, target: &Path, module: SoongModule) -> Result<SoongModule, String> {
        let is_soc_specific = |module: SoongModule| -> SoongModule {
            for lib in [
                "libEGL_mesa.so",
                "libGLESv1_CM_mesa.so",
                "libGLESv2_mesa.so",
                "dri_gbm.so",
                "libgallium_dri.so",
                "libgbm_mesa.so",
                "libvulkan_panfrost.so",
            ] {
                if target.ends_with(lib) {
                    return module.add_prop("soc_specific", SoongProp::Bool(true));
                }
            }
            module
        };
        let module = is_soc_specific(module);

        let relative_install = |module: SoongModule| -> SoongModule {
            for lib in [
                "libEGL_mesa.so",
                "libGLESv1_CM_mesa.so",
                "libGLESv2_mesa.so",
            ] {
                if target.ends_with(lib) {
                    return module
                        .add_prop("relative_install_path", SoongProp::Str(String::from("egl")));
                }
            }
            if target.ends_with("libvulkan_panfrost.so") {
                return module
                    .add_prop("relative_install_path", SoongProp::Str(String::from("hw")));
            }
            module
        };
        let module = relative_install(module);

        let header_libs = |module: SoongModule| -> SoongModule {
            for header_lib in [
                "libdri.a",
                "libgallium.a",
                "libkmsrowinsys.a",
                "libloader.a",
                "libmesa_util.a",
                "libpipe_loader_dynamic.a",
                "libpipe_loader_static.a",
                "libswkmsdri.a",
                "libpanfrost_perf.a",
                "libpanfrost_midgard_disasm.a",
                "libpanfrost_midgard.a",
                "libpanfrost_shared.a",
                "libpanfrost_bifrost_disasm.a",
                "libpanfrost_bifrost.a",
                "libpanfrost_valhall_disasm.a",
                "libpanfrost_decode.a",
                "libpanfrost_lib.a",
                "libpanfrost_util.a",
                "libvulkan_instance.a",
                "libvulkan_lite_runtime.a",
                "libvulkan_runtime.a",
                "libvulkan_wsi.a",
            ] {
                if target.ends_with(header_lib) {

                    if target.ends_with("libvulkan_lite_runtime.a")
                    {
                        return module.add_prop(
                            "header_libs",
                            SoongProp::VecStr(vec![
                                String::from("hwvulkan_headers"),
                                String::from("libdrm_headers"),
                            ]),
                        );
                    } else {
                        return module.add_prop(
                            "header_libs",
                            SoongProp::VecStr(vec![String::from("libdrm_headers")]),
                        );
                    }
                }
            }

            if target.ends_with("libEGL_mesa.so") {
                return module.add_prop(
                    "header_libs",
                    SoongProp::VecStr(vec![String::from("libnativebase_headers")]),
                );
            }
            module
        };
        let module = header_libs(module);

        let export_include_dirs = |module: SoongModule| -> SoongModule {
            if target.ends_with("libgbm_mesa.so") {
                return module.add_prop(
                    "export_include_dirs",
                    SoongProp::VecStr(vec![String::from("src/gbm/main")]),
                );
            }
            module
        };
        let module = export_include_dirs(module);

        let mut cflags = vec![
            "-Wno-constant-conversion",
            "-Wno-enum-conversion",
            "-Wno-error",
            "-Wno-ignored-qualifiers",
            "-Wno-initializer-overrides",
            "-Wno-macro-redefined",
            "-Wno-non-virtual-dtor",
            "-Wno-pointer-arith",
            "-Wno-unused-parameter",
        ];
        if target.ends_with("libnir.a") {
            cflags.push("-Wno-bool-conversion");
        }
        if target.ends_with("libvulkan_lite_runtime.a")
        {
            cflags.push("-Wno-unreachable-code-loop-increment");
        }
        if target.ends_with("lib_mesa_u_gralloc.a") {
            cflags.push("-DUSE_IMAPPER4_METADATA_API");
        }

        let mut libs = Vec::new();
        if target.ends_with("libdri.a")
            || target.ends_with("libgallium.a")
            || target.ends_with("libvulkan_lite_runtime.a")
            || target.ends_with("libvulkan_wsi.a")
        {
            libs.push("libsync");
        }
        if target.ends_with("libmesa_util.a") {
            libs.push("libz");
        }
        if target.starts_with("src/panfrost/vulkan") || target.ends_with("libvulkan_lite_runtime.a")
        {
            libs.push("libnativewindow");
        }
        if target.ends_with("libEGL_mesa.so")
            || target.ends_with("libvulkan_panfrost.so")
            || target.ends_with("lib_mesa_u_gralloc.a")
        {
            libs.push("libui");
        }

        let mut sources = Vec::new();
        if target.ends_with("lib_mesa_u_gralloc.a") {
            sources.push("src/util/u_gralloc/u_gralloc_imapper5_api.cpp");
        }

        module
            .add_prop("defaults", SoongProp::VecStr(vec![String::from(DEFAULTS)]))
            .extend_prop("cflags", cflags)?
            .extend_prop("shared_libs", libs)?
            .extend_prop("srcs", sources)
    }
    fn map_lib(&self, library: &Path) -> Option<PathBuf> {
        if library.starts_with("src/android_stub") || !library.starts_with("src") {
            Some(PathBuf::from(file_stem(library)))
        } else {
            None
        }
    }
    fn filter_cflag(&self, cflag: &str) -> bool {
        !cflag.starts_with("'") && cflag != "-fno-rtti"
    }
    fn filter_define(&self, _define: &str) -> bool {
        true
    }
    fn filter_include(&self, include: &Path) -> bool {
        let inc = path_to_string(include);
        let subprojects = self.src_path.join("subprojects");
        !include.ends_with("android_stub") && !inc.contains(&path_to_string(&subprojects))
    }
    fn filter_link_flag(&self, flag: &str) -> bool {
        flag == "-Wl,--build-id=sha1" || flag == "-Wl,-Bsymbolic"
    }
    fn filter_gen_header(&self, _header: &Path) -> bool {
        false
    }
    fn filter_lib(&self, _lib: &str) -> bool {
        true
    }
    fn filter_target(&self, target: &Path) -> bool {
        let file_name = file_name(target);
        !file_name.ends_with(".o")
            && !file_name.ends_with(".def")
            && !file_name.contains("libdrm")
            && !target.starts_with("src/android_stub")
    }
}
