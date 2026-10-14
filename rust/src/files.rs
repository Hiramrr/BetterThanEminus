#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileKind {
    Pdf,
    Document,
    Spreadsheet,
    Presentation,
    Image,
    Archive,
    Code,
    Text,
    Audio,
    Video,
    Other,
}

impl FileKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Pdf => "PDF",
            Self::Document => "documento",
            Self::Spreadsheet => "hoja de cálculo",
            Self::Presentation => "presentación",
            Self::Image => "imagen",
            Self::Archive => "archivo comprimido",
            Self::Code => "código",
            Self::Text => "texto",
            Self::Audio => "audio",
            Self::Video => "video",
            Self::Other => "archivo",
        }
    }

    pub fn badge(self) -> &'static str {
        match self {
            Self::Pdf => "PDF",
            Self::Document => "DOC",
            Self::Spreadsheet => "XLS",
            Self::Presentation => "PPT",
            Self::Image => "IMG",
            Self::Archive => "ZIP",
            Self::Code => "</>",
            Self::Text => "TXT",
            Self::Audio => "AUD",
            Self::Video => "VID",
            Self::Other => "FILE",
        }
    }
}

pub fn kind(name: &str) -> FileKind {
    let name = name.trim().rsplit(['/', '\\']).next().unwrap_or_default();
    let extension = name.rsplit_once('.').map(|(_, ext)| ext.to_ascii_lowercase()).unwrap_or_default();
    match extension.as_str() {
        "pdf" => FileKind::Pdf,
        "doc" | "docx" | "odt" | "rtf" | "pages" => FileKind::Document,
        "xls" | "xlsx" | "xlsm" | "ods" | "csv" | "tsv" | "numbers" => FileKind::Spreadsheet,
        "ppt" | "pptx" | "odp" | "key" => FileKind::Presentation,
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "svg" | "bmp" | "tiff" | "tif" | "heic" | "avif" | "ico" => FileKind::Image,
        "zip" | "rar" | "7z" | "tar" | "gz" | "bz2" | "xz" | "tgz" | "zst" => FileKind::Archive,
        "rs" | "py" | "js" | "jsx" | "ts" | "tsx" | "java" | "c" | "cpp" | "h" | "cs" | "go" | "rb" | "php" | "swift" | "kt" | "html" | "css" | "sql" | "json" | "xml" | "yaml" | "yml" | "toml" | "sh" | "ipynb" => FileKind::Code,
        "txt" | "md" | "markdown" | "log" => FileKind::Text,
        "mp3" | "wav" | "ogg" | "flac" | "m4a" | "aac" | "opus" | "aiff" => FileKind::Audio,
        "mp4" | "mkv" | "mov" | "avi" | "webm" | "m4v" | "wmv" => FileKind::Video,
        _ => FileKind::Other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_real_filenames_without_changing_them() {
        for (name, expected) in [
            ("  Informe final.PDF  ", FileKind::Pdf),
            ("C:\\clase.pdf\\tarea.DOCX", FileKind::Document),
            ("/tmp/foto.png/tabla.csv", FileKind::Spreadsheet),
            ("diapositivas.odp", FileKind::Presentation),
            ("foto.HEIC", FileKind::Image),
            ("código.tar.gz", FileKind::Archive),
            ("reporte.ipynb", FileKind::Code),
            ("lectura.md", FileKind::Text),
            ("grabación.ogg", FileKind::Audio),
            ("clase.webm", FileKind::Video),
            ("archivo", FileKind::Other),
            ("informe.pdf.exe", FileKind::Other),
            ("archivo.", FileKind::Other),
        ] {
            assert_eq!(kind(name), expected, "{name}");
        }
    }
}
