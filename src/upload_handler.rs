use std::fs;
use std::io::Write;
use std::path::Path;
use std::str::from_utf8;


pub fn handle_multipart(body: &[u8], content_type: &str) -> std::io::Result<()> {
    println!("In handle");
    let boundary = extract_boundary(content_type)
        .ok_or(std::io::Error::new(std::io::ErrorKind::Other, "No boundary"))?;

    let boundary_marker = format!("--{}", boundary);
    let boundary_bytes = boundary_marker.as_bytes();

    let mut cursor = 0;

    while let Some(pos) = find_subslice(&body[cursor..], boundary_bytes) {
        let start = cursor + pos + boundary_bytes.len();

        // Skip CRLF
        let mut i = start;
        if body.get(i..i+2) == Some(b"\r\n") {
            i += 2;
        }

        // Find end of headers
        if let Some(header_end_rel) = find_subslice(&body[i..], b"\r\n\r\n") {
            let header_end = i + header_end_rel;
            let headers = &body[i..header_end];

            let data_start = header_end + 4;

            // Find next boundary
            if let Some(next_rel) = find_subslice(&body[data_start..], boundary_bytes) {
                let data_end = data_start + next_rel - 2;

                let filename_marker = b"filename=\"";

                if headers.windows(filename_marker.len())
                        .any(|w| w == filename_marker)
                {
                    if let Some(filename) = extract_filename(headers) {
                        println!("Filename detected: {}", filename);
                        save_file(&filename, &body[data_start..data_end])?;
                    }
                }

                cursor = data_start + next_rel;
            } else {
                break;
            }
        } else {
            break;
        }
    }

    Ok(())
}

fn extract_boundary(content_type: &str) -> Option<String> {
    content_type.split(';')
        .find_map(|s| {
            let s = s.trim();
            if s.starts_with("boundary=") {
                Some(s.trim_start_matches("boundary=").to_string())
            } else {
                None
            }
        })
}

fn find_subslice(h: &[u8], n: &[u8]) -> Option<usize> {
    h.windows(n.len()).position(|w| w == n)
}

fn extract_filename(headers: &[u8]) -> Option<String> {
    let s = String::from_utf8_lossy(headers);
    let start = s.find("filename=\"")? + 10;
    let end = s[start..].find('"')?;
    Some(s[start..start+end].to_string())
}

fn save_file(
    original_filename: &str,
    file_bytes: &[u8],
) -> std::io::Result<()> {
    println!("In save function");
    let extension = Path::new(original_filename)
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("bin");

    let medias_dir = "server/medias/";

    // Calc the number of file
    let mut max_index = 0;
    for entry in fs::read_dir(medias_dir)? {
        let entry = entry?;
        eprintln!("{:?}", entry);
        if let Some(file_name) = entry.file_name().to_str() {
            if let Some(stem) = Path::new(file_name).file_stem() {
                if let Some(stem_str) = stem.to_str() {
                    if let Ok(num) = stem_str.parse::<u32>() {
                        if num > max_index {
                            max_index = num;
                        }
                    }
                }
            }
        }
    }

    let new_index = max_index + 1;

    // Renaming file
    let new_filename = format!("{}.{}", new_index, extension);
    let full_path = format!("{}/{}", medias_dir, new_filename);
    // Writing file
    let mut file = fs::File::create(full_path)?;
    file.write_all(file_bytes)?;

    Ok(())
}