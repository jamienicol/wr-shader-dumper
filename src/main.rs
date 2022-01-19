use std::{
    borrow::Cow,
    fs::{create_dir_all, File},
    io::Write,
    path::Path,
};

use webrender_build::{
    shader::{build_shader_strings, shader_source_from_file, ShaderVersion},
    shader_features::{get_shader_features, ShaderFeatureFlags},
};

#[derive(Clone, Debug)]
struct ShaderOptimizationInput {
    shader_name: &'static str,
    config: String,
    gl_version: ShaderVersion,
}

#[derive(Debug)]
#[allow(dead_code)]
struct ShaderOptimizationError {
    shader: ShaderOptimizationInput,
    message: String,
}

fn write_shader_files(vert_src: &str, frag_src: &str, out_dir: &Path, base_filename: &str) {
    let vert_file_path = Path::new(out_dir).join(format!("{}.vert", base_filename));
    let mut vert_file = File::create(&vert_file_path).unwrap();
    vert_file.write_all(vert_src.as_bytes()).unwrap();

    let frag_file_path = vert_file_path.with_extension("frag");
    let mut frag_file = File::create(&frag_file_path).unwrap();
    frag_file.write_all(frag_src.as_bytes()).unwrap();
}

fn main() {
    let shader_versions = [ShaderVersion::Gl, ShaderVersion::Gles];

    let mut shaders = Vec::default();
    for &gl_version in &shader_versions {
        let mut flags = ShaderFeatureFlags::all();
        if gl_version != ShaderVersion::Gl {
            flags.remove(ShaderFeatureFlags::GL);
        }
        if gl_version != ShaderVersion::Gles {
            flags.remove(ShaderFeatureFlags::GLES);
            flags.remove(ShaderFeatureFlags::TEXTURE_EXTERNAL);
            flags.remove(ShaderFeatureFlags::TEXTURE_EXTERNAL_ESSL1);
        }

        for (shader_name, configs) in get_shader_features(flags) {
            for config in configs {
                shaders.push(ShaderOptimizationInput {
                    shader_name,
                    config,
                    gl_version,
                });
            }
        }
    }

    let input_dir = Path::new("/home/jamie/src/gecko/gfx/wr/webrender/res");
    let unopt_out_dir = Path::new("/home/jamie/shaders/orig");
    create_dir_all(unopt_out_dir).unwrap();
    let opt_out_dir = Path::new("/home/jamie/shaders/glslopt");
    create_dir_all(opt_out_dir).unwrap();

    build_parallel::compile_objects(
        &|shader: &ShaderOptimizationInput| {
            println!("Optimizing shader {:?}", shader);
            let target = match shader.gl_version {
                ShaderVersion::Gl => glslopt::Target::OpenGl,
                ShaderVersion::Gles => glslopt::Target::OpenGles30,
            };
            let glslopt_ctx = glslopt::Context::new(target);

            let features = shader
                .config
                .split(',')
                .filter(|f| !f.is_empty())
                .collect::<Vec<_>>();

            let full_shader_name = if shader.config.is_empty() {
                shader.shader_name.to_string()
            } else {
                format!("{}_{}", shader.shader_name, shader.config.replace(",", "_"))
            };
            let base_filename = format!("{}_{:?}", full_shader_name, shader.gl_version);

            let (vert_src, frag_src) =
                build_shader_strings(shader.gl_version, &features, shader.shader_name, &|f| {
                    Cow::Owned(shader_source_from_file(
                        &input_dir.join(&format!("{}.glsl", f)),
                    ))
                });

            write_shader_files(&vert_src, &frag_src, unopt_out_dir, &base_filename);

            let vert = glslopt_ctx.optimize(glslopt::ShaderType::Vertex, vert_src);
            if !vert.get_status() {
                return Err(ShaderOptimizationError {
                    shader: shader.clone(),
                    message: vert.get_log().to_string(),
                });
            }
            let frag = glslopt_ctx.optimize(glslopt::ShaderType::Fragment, frag_src);
            if !frag.get_status() {
                return Err(ShaderOptimizationError {
                    shader: shader.clone(),
                    message: frag.get_log().to_string(),
                });
            }

            let vert_output = vert.get_output().unwrap();
            let frag_output = frag.get_output().unwrap();

            write_shader_files(vert_output, frag_output, opt_out_dir, &base_filename);

            Ok(())
        },
        &shaders,
    )
    .unwrap();
}
