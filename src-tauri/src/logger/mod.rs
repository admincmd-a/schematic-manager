use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::{PathBuf};
use std::sync::{Mutex, OnceLock};
use chrono::{Local, NaiveDate};
use std::panic::Location;
#[cfg(target_os = "windows")]
use std::os::windows::fs::OpenOptionsExt;

use crate::message_window::MessageWindow;

const LOG_FILE_NAME: &str = "_latest.log";
const LOG_DEBUG_FILE_NAME: &str = "debug.log";

/// 单条日志最长字符长度
const LOG_ITEM_MAX_CHAR_LENGTH: usize = 0xFFFF;
const LOG_ARCHIVES_MAX_ITEM: usize = 100;

#[derive(PartialEq, Eq, Debug, Clone, Copy)]
/// 等级
pub enum LEVEL {
    FATAL,
    ERROR,
    WARN,
    INFO,
    DEBUG,
    UNKNOWN,
}

impl LEVEL {
    pub fn parse_level_class(name: &str) -> LEVEL {
        match name.trim().to_ascii_uppercase().as_str() {
            "FATAL" => return LEVEL::FATAL,
            "ERROR" => return LEVEL::ERROR,
            "WARN"  => return LEVEL::WARN,
            "INFO"  => return LEVEL::INFO,
            "DEBUG" => return LEVEL::DEBUG,
            _       => return LEVEL::UNKNOWN,
        }
    }

    pub fn get_level_str(name: LEVEL) -> & 'static str {
        match name {
            LEVEL::FATAL    => return "FATAL",
            LEVEL::ERROR    => return "ERROR",
            LEVEL::WARN     => return "WARN" ,
            LEVEL::INFO     => return "INFO" ,
            LEVEL::DEBUG    => return "DEBUG",
            LEVEL::UNKNOWN  => return "UNKNOWN",
        }
    }
}
pub trait LoggerGlobal {
    #[track_caller]
    fn debug(&self, message: &str);

    #[track_caller]
    fn info(&self, message: &str);

    #[track_caller]
    fn warn(&self, message: &str);

    #[track_caller]
    fn error(&self, message: &str);

    #[track_caller]
    fn fatal(&self, message: &str);
}

impl LoggerGlobal for OnceLock<LogFileObject> {
    #[track_caller]
    fn info(&self, message: &str) {
        if let Some(logger) = self.get() {
            logger.info(message);
        }
    }

    #[track_caller]
    fn warn(&self, message: &str) {
        if let Some(logger) = self.get() {
            logger.warn(message);
        }
    }

    #[track_caller]
    fn debug(&self, message: &str) {
        if let Some(logger) = self.get() {
            logger.debug(message);
        }
    }

    #[track_caller]
    fn error(&self, message: &str) {
        if let Some(logger) = self.get() {
            logger.error(message);
        }
    }

    #[track_caller]
    fn fatal(&self, message: &str) {
        if let Some(logger) = self.get() {
            logger.fatal(message);
        }
    }
}

/// 取到当前的可读时间戳
pub fn get_now_time() -> String {
    Local::now().format("%Y-%m-%d %H:%M:%S%.6f").to_string()
}

/// 取得当前可读日期
fn get_now_date() -> String {
    Local::now().format("%Y-%m-%d").to_string()
}

/// 归档文件名：`<日期>-<序号>-<类型>.log.gz`。
///
/// 序号全局递增
fn archive_file_name(date: &str, index: u64, base_name: &str) -> String {
    format!("{}-{}-{}.log.gz", date, index, base_name)
}

/// 解析本类归档的文件名，取出日期与序号；不是本类归档时返回 `None`。
///
/// 通过解析日期格式 `YYYY-MM-DD`、文件名后缀以及文件扩展名完成
/// AIGC
fn parse_archive_name(name: &str, base_name: &str) -> Option<(NaiveDate, u64)> {
    let stem = name.strip_suffix(".log.gz")?;
    let stem = stem.strip_suffix(&format!("-{}", base_name))?;
    let (date, index) = stem.rsplit_once('-')?;
    let date = NaiveDate::parse_from_str(date, "%Y-%m-%d").ok()?;
    Some((date, index.parse().ok()?))
}

/// 目录里已有本类归档的最大序号；没有归档时返回 0。
/// AIGC
fn get_max_archive_index(base_name: &str, dir: &PathBuf) -> u64 {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            parse_archive_name(&name, base_name).map(|(_, index)| index)
        })
        .max()
        .unwrap_or(0)
}

/// 下一个归档路径：序号取已有归档的最大值加一。
fn get_next_archive_name(base_name: &str, dir: &PathBuf) -> PathBuf {
    let mut n = get_max_archive_index(base_name, dir) + 1;
    loop {
        let candidate = dir.join(archive_file_name(&get_now_date(), n, base_name));
        if !candidate.exists() {
            return candidate;
        }
        n += 1;
    }
}

pub struct LogFileObject {
    #[allow(dead_code)]
    log_dir: PathBuf,
    latest_writer: Mutex<BufWriter<File>>,
    debug_writer: Mutex<BufWriter<File>>,
}

/// 一个文件日志系统对象
impl LogFileObject {

    /// 标准初始化流程，生产使用
    pub fn init(run_path: &PathBuf) -> Self {
        match Self::try_init(run_path) {
            Ok(this) => this,
            Err(error_str) => {
                let _ = MessageWindow::new()
                    .set_icon(crate::message_window::MessageWindowIcon::ERROR)
                    .set_title("Log System Error")
                    .set_content(&error_str)
                    .show();
                std::process::exit(0x6);
            }
        }
    }

    /// 初始化新的日志
    pub fn try_init(run_path: &PathBuf) -> Result<Self, String> {
        let log_dir = run_path.clone();

        // 线保证日志目录一定存在
        if let Err(e) = std::fs::create_dir_all(&log_dir) {
            return Err(format!("Failed to create log directory: \n{}", e));
        }

        // 然后再去吧日志滚走
        match LogFileObject::roll_old_log_file(&log_dir) {
            Ok(()) => println!("Log system initialization: archived old log files."),
            Err(e) => {
                println!("[{}][ERROR][::logger] {}", get_now_time(), e);
            },
        };

        LogFileObject::clean_old_archives(&log_dir, "latest", LOG_ARCHIVES_MAX_ITEM);
        LogFileObject::clean_old_archives(&log_dir, "debug", LOG_ARCHIVES_MAX_ITEM);

        let mut file_opts = OpenOptions::new();
        file_opts.create(true).append(true);

        #[cfg(target_os = "windows")]
        {file_opts.share_mode(0x1);} // 共享读

        // 再去创建文件
        let latest_file = match file_opts.open(&log_dir.join(LOG_FILE_NAME)) {
            Ok(file) => file,
            Err(e) => {
                return Err(format!("The latest.log initialization failed and the log file could not be saved: \n{}", e));
            },
        };
        let debug_file  = match file_opts.open(&log_dir.join(LOG_DEBUG_FILE_NAME)) {
            Ok(file) => file,
            Err(e) => {
                return Err(format!("The debug.log initialization failed and the log file could not be saved: \n{}", e));
            },
        };

        // 然后再把自己搞回去
        Ok(Self {
            log_dir: run_path.clone(),
            latest_writer: Mutex::new(BufWriter::new(latest_file)),
            debug_writer: Mutex::new(BufWriter::new(debug_file)),
        })
    }

    /// 将上次的日志压缩归档
    fn roll_old_log_file(dir: &PathBuf) -> Result<(), std::io::Error> {
        fn roll_file(name: &str, base_name: &str, dir: &PathBuf) -> Result<(), std::io::Error> {
            let file_current = dir.join(name);
            if !file_current.exists() { return Ok(()); } // 忽略 ~~404~~ NotFound
            let file_data = std::fs::read(&file_current)?;
            let file_packed_name = get_next_archive_name(base_name, dir);

            let mut out_packed_encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
            out_packed_encoder.write_all(&file_data)?;
            let out_packed = out_packed_encoder.finish()?;

            std::fs::write(&file_packed_name, out_packed)?;
            std::fs::File::create(&file_current)?; // 创建文件
            Ok(())
        }

        roll_file( LOG_FILE_NAME, "latest", dir)?;
        roll_file( LOG_DEBUG_FILE_NAME, "debug", dir)?;
        
        Ok(())
    }

    /// 清理旧的归档日志，只保留最近的 `keep_count` 个包
    fn clean_old_archives(dir: &PathBuf, base_name: &str, keep_count: usize) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };

        let mut archives: Vec<(NaiveDate, u64, PathBuf)> = entries
            .flatten()
            .filter_map(|entry| {
                let name = entry.file_name().to_string_lossy().into_owned();
                let (date, index) = parse_archive_name(&name, base_name)?;
                Some((date, index, entry.path()))
            })
            .collect();

        archives.sort_by_key(|(date, index, _)| (*date, *index));

        // 查总量
        if archives.len() > keep_count {
            let to_delete = archives.len() - keep_count;
            for (_, _, path) in archives.iter().take(to_delete) {
                if let Err(e) = std::fs::remove_file(path) {
                    println!("[WARN] 无法删除旧日志归档 {}: {}", path.display(), e);
                }
            }
        }
    }

    #[track_caller]
    /// 写日志，该函数适合一般调用
    pub fn write_log(&self, level: LEVEL, message: &str) {
        Self::write_log_caller(&self, level, message, Location::caller());
    }

    #[track_caller]
    /// 写日志，实例函数
    /// # 参数
    /// - `level` - 使用 `LEVEL` 的等级，
    /// - `message` - 正文
    /// - `caller` - `Location::caller()` 的返回
    pub fn write_log_caller(&self, level: LEVEL, message: &str, caller: &'static Location<'static>) {
        if message.is_empty() {return;}

        let out_message = message;
        let time = get_now_time();
        
        let log_length = out_message.chars().count();
        let out_message = if log_length > LOG_ITEM_MAX_CHAR_LENGTH {
            out_message.chars().take(LOG_ITEM_MAX_CHAR_LENGTH).collect::<String>()
        } else {
            String::from(out_message)
        };
        let mut out_text = format!(
            "[{}][{}][{}:{}] {}",
             time,
             LEVEL::get_level_str(level),
             caller.file(),
             caller.line(),
             out_message,
        );
        
        println!("{}", out_text);
        if log_length > LOG_ITEM_MAX_CHAR_LENGTH {
            let log = format!(
                "[{}][WARN][{}:{}][LogSystem][Log Long] ----- The log is too long and has been forced to truncate. The maximum value is {}, the length of this log {}. Note: Try to dump the content you want to print instead of exporting it to the log."
                , time
                , caller.file()
                , caller.line()
                , LOG_ITEM_MAX_CHAR_LENGTH
                , log_length
            );
            println!("{}", log);
            out_text = out_text + "\n" + &log
        }

        if LEVEL::DEBUG != level {
            let mut writer_latest = self.latest_writer.lock().unwrap_or_else(|e| e.into_inner());
            if let Err(e) = writeln!(writer_latest, "{}", out_text) {
                eprintln!(
                    "[{}][ERROR][{}:{}][LogSystem][Log Write] ----- An error occurred while trying to write to latest.log: {}"
                    , get_now_time()
                    , file!()
                    , line!()
                    , e
                )
            }
            let _ = writer_latest.flush();
        }

        let mut writer_debug = self.debug_writer.lock().unwrap_or_else(|e| e.into_inner());
        if let Err(e) = writeln!(writer_debug, "{}", out_text) {
            eprintln!(
                "[{}][ERROR][{}:{}][LogSystem][Log Write] ----- An error occurred while trying to write to debug.log: {}"
                , get_now_time()
                , file!()
                , line!()
                , e
            )
        }

        let _ = writer_debug.flush(); // 刷新
    }

    #[track_caller]
    pub fn debug(&self, message: &str) {
        Self::write_log_caller(&self, LEVEL::DEBUG, message, Location::caller());
    }

    #[track_caller]
    pub fn info(&self, message: &str) {
        Self::write_log_caller(&self, LEVEL::INFO, message, Location::caller());
    }

    #[track_caller]
    pub fn warn(&self, message: &str) {
        Self::write_log_caller(&self, LEVEL::WARN, message, Location::caller());
    }

    #[track_caller]
    pub fn error(&self, message: &str) {
        Self::write_log_caller(&self, LEVEL::ERROR, message, Location::caller());
    }

    #[track_caller]
    pub fn fatal(&self, message: &str) {
        Self::write_log_caller(&self, LEVEL::FATAL, message, Location::caller());
    }
}

/////////////////////////////////////////
// ----------------------------------- //
/////////////////////////////////////////

/// 替换字符串中的 `{}` 和 `%s`
/// AICG
/// 参考代码
/// 保留代码，用作 `%s` 的处理上，并且不应当替代 `format` 宏
#[allow(dead_code)]
pub fn replace_template_placeholders_manual(template: &str, args: &[&str]) -> String {
    let mut result = String::with_capacity(template.len());
    let mut chars = template.chars().peekable();
    let mut arg_index = 0;

    while let Some(c) = chars.next() {
        if c == '{' {
            if let Some(&'}') = chars.peek() {
                chars.next(); // 消耗掉 '}'
                if arg_index < args.len() {
                    result.push_str(args[arg_index]);
                    arg_index += 1;
                } else {
                    result.push_str("{}"); // 参数不够，原样保留
                }
            } else {
                result.push(c); // 不是 {}，原样保留
            }
        } else if c == '%' {
            if let Some(&'s') = chars.peek() {
                chars.next(); // 消耗掉 's'
                if arg_index < args.len() {
                    result.push_str(args[arg_index]);
                    arg_index += 1;
                } else {
                    result.push_str("%s"); // 参数不够，原样保留
                }
            } else {
                result.push(c);
            }
        } else {
            result.push(c);
        }
    }
    result
}


/// 入口函数
/// 入口
pub fn init_log_object(dir: &PathBuf) -> () {
    let _ = LOGGER.get_or_init(|| LogFileObject::init(dir));
    ()
}

////////////////////////////////////////////
/// -----------------------------------  ///
////////////////////////////////////////////

// 创建一个全局唯一并且还互斥还不会是空的元素
pub static LOGGER: OnceLock<LogFileObject> = OnceLock::new();

#[cfg(test)]
#[path = "../tests/logger/mod.rs"]
mod tests;