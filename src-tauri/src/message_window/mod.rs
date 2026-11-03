#[cfg(target_os = "windows")]
use windows::Win32::UI::Controls::{TASKDIALOG_COMMON_BUTTON_FLAGS, TDCBF_CANCEL_BUTTON, TDCBF_CLOSE_BUTTON, TDCBF_NO_BUTTON, TDCBF_RETRY_BUTTON, TDCBF_YES_BUTTON};
#[cfg(target_os = "windows")]
use windows::core::{HSTRING, PCWSTR};
#[cfg(target_os = "windows")]
use windows::Win32::Foundation::{HWND};
#[cfg(target_os = "windows")]
use windows::core::BOOL;
#[cfg(target_os = "windows")]
use windows::Win32::UI::Controls::{
    TaskDialogIndirect, TASKDIALOGCONFIG, TASKDIALOGCONFIG_0, TASKDIALOGCONFIG_1,
    TASKDIALOG_BUTTON, TDCBF_OK_BUTTON, TDF_ALLOW_DIALOG_CANCELLATION, TD_INFORMATION_ICON,
};
#[cfg(target_os = "windows")]
use windows::Win32::UI::WindowsAndMessaging::HICON;

#[cfg(not(target_os = "windows"))]
use rfd::{MessageButtons, MessageDialog, MessageDialogResult, MessageLevel};

use crate::logger::LEVEL;
use std::panic::Location;


/// 由于 rfd::MessageDialog 不支持太多窗口按钮类型组合，导致能用的只有这么几个 ------ @see <https://docs.rs/rfd/latest/rfd/struct.MessageDialog.html>
/// // rfd::MessageButtons

/// 如果想知道 TaskDialog 的参数，参见下面
/// @see <https://learn.microsoft.com/zh-cn/windows/win32/api/commctrl/ns-commctrl-taskdialogconfig> 

/// TaskDialog 所使用的基本按钮，自定义 ------ @see <https://learn.microsoft.com/zh-cn/windows/win32/api/commctrl/nf-commctrl-taskdialog>
pub enum MessageWindowButton {
    ABORT,
    CANCEL,
    CLOSE,
    CONTINUE,
    HELP,
    IGNORE,
    NO,
    OK,
    RETRY,
    TRYAGAIN,
    YES,
    Custom(i32, String),
}

/// 返回值
pub enum MessageWindowDialog {
    NULL,
    ABORT,
    CANCEL, // 根据 Win32 文档，此处为 Escape 返回
    CLOSE,
    CONTINUE,
    HELP,
    IGNORE,
    NO,
    OK,
    RETRY,
    TRYAGAIN,
    YES,
    Custom(i32),
}

/// 图标类型，基于 Windows 设计，不支持的平台将会转义/抛弃
#[non_exhaustive]
pub enum MessageWindowIcon {
    NULL,
    ERROR,
    SHIELD,
    WARN,
    INFO,
    #[cfg(windows)]
    HICON(HICON), // 我们的建议是最好在 WINDOWS 上拿这玩意引软件 ICO
}

#[derive(Debug)]
pub enum MessageWindowError {
    /// Windows 上 TaskDialog 自身的失败。非 Windows 平台没有 windows crate，故不参与编译。
    #[cfg(target_os = "windows")]
    WindowsSystem(windows::core::Error),    // Win32
    TooManyButtons(usize),           // 非 Windows 上按钮超过 3 个
}

pub struct MessageWindow {
    icon: MessageWindowIcon,
    buttons: Vec<MessageWindowButton>,
    title: String,
    instruction: String,
    content: String,
    expanded_info: String
}

impl std::fmt::Display for MessageWindowError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::WindowsSystem(e) => write!(f, "Windows API 调用失败: {e}"),
            Self::TooManyButtons(n) => write!(f, "按钮数量 {n} 超出非 Windows 平台的上限（最多 3 个）"),
        }
    }
}

impl std::error::Error for MessageWindowError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::WindowsSystem(e) => Some(e),
            _ => None,
        }
    }
}

impl MessageWindow {
    /// New Self
    pub fn new() -> Self {
        Self {
            icon: MessageWindowIcon::NULL,
            buttons: Vec::with_capacity(3),
            title: String::new(),
            instruction: String::new(),
            content: String::new(),
            expanded_info: String::new(),

        }
    }

    /// 设置图标
    /// - 非 Windows 降级逻辑：NULL | SHIELD | INFO => INFO
    pub fn set_icon(mut self, icon: MessageWindowIcon) -> Self {
        self.icon = icon;
        self
    }

    /// 使用抽象格式设置窗口按钮组合
    /// - 在非 Windows 最多支持 3 个按钮（受 rfd 限制），故我们不建议您传入 >3 个按钮，若调用仅在 Windows 上发生，也可以处理超过 3 按钮的组合
    /// - 如果没有设置按钮，依照 Win32 默认值，使用确定按钮
    pub fn set_buttons(mut self, buttons: Vec<MessageWindowButton>) -> Self {
        self.buttons = buttons;
        self
    }

    /// 使用 rfd 格式设置窗口按钮组合
    pub fn set_rfd_buttons(mut self, buttons: rfd::MessageButtons) -> Self {
        self.buttons = Self::buttons_to_button(buttons);
        self
    }

    fn buttons_to_button(buttons: rfd::MessageButtons) -> Vec<MessageWindowButton> {
        match buttons {
            rfd::MessageButtons::Ok          => vec![MessageWindowButton::OK],
            rfd::MessageButtons::OkCancel    => vec![MessageWindowButton::OK, MessageWindowButton::CANCEL],
            rfd::MessageButtons::YesNo       => vec![MessageWindowButton::YES, MessageWindowButton::NO],
            rfd::MessageButtons::YesNoCancel => vec![MessageWindowButton::YES, MessageWindowButton::NO, MessageWindowButton::CANCEL],

            rfd::MessageButtons::OkCustom(ok_string)                                                     => vec![MessageWindowButton::Custom(100, ok_string)],
            rfd::MessageButtons::OkCancelCustom(ok_string, cancel_string)                        => vec![MessageWindowButton::Custom(100, ok_string), MessageWindowButton::Custom(101, cancel_string)],
            rfd::MessageButtons::YesNoCancelCustom(yes_string, no_string, cancel_string) => vec![MessageWindowButton::Custom(100, yes_string), MessageWindowButton::Custom(101, no_string), MessageWindowButton::Custom(102, cancel_string)],
        }
    }

    /// 设置窗口标题
    pub fn set_title(mut self, title: &str) -> Self {
        self.title = title.to_string();
        self
    }

    /// 仅 Windows 原生支持
    /// 设置大大的蓝色主字，非 Windows 会给你放到最前面
    pub fn set_instruction(mut self, instruction: &str) -> Self {
        self.instruction = instruction.to_string();
        self
    }

    /// 主体内容
    pub fn set_content(mut self, content: &str) -> Self {
        self.content = content.to_string();
        self
    }

    /// 仅 Windows 原生支持，非 Windows 不折叠
    /// 用于显示详细信息，设置后会自动添加 折叠/打开 按钮
    pub fn set_expanded_info(mut self, expanded_info: &str) -> Self {
        self.expanded_info = expanded_info.to_string();
        self
    }

    /// 尝试在 Windows 上使用 TaskDialog 显示窗口
    #[cfg(target_os = "windows")]
    #[track_caller]
    pub fn show(self) -> Result<MessageWindowDialog, MessageWindowError> {
        use windows::Win32::UI::Controls::{TD_ERROR_ICON, TD_SHIELD_ICON, TD_WARNING_ICON};
        use windows::Win32::UI::Controls::{TDCBF_ABORT_BUTTON, TDF_USE_HICON_MAIN, TDCBF_CONTINUE_BUTTON, TDCBF_HELP_BUTTON, TDCBF_IGNORE_BUTTON, TDCBF_TRYAGAIN_BUTTON};
        use windows::Win32::UI::WindowsAndMessaging::{MESSAGEBOX_RESULT, IDABORT, IDCANCEL, IDNO, IDCLOSE, IDCONTINUE, IDHELP, IDIGNORE, IDOK, IDRETRY, IDTRYAGAIN, IDYES};

        let mut p_buttons = vec![];
        let mut c_buttons = TASKDIALOG_COMMON_BUTTON_FLAGS(0);
        let mut button_texts: Vec<HSTRING> = Vec::new(); // 文本指针指向，目的是为了防止提前释放内存导致的非预期行为
        let mut flags = TDF_ALLOW_DIALOG_CANCELLATION;

        let content = HSTRING::from(self.content);
        let title = HSTRING::from(self.title);
        let instruction = HSTRING::from(self.instruction);
        let expanded_info = if self.expanded_info.is_empty() {None} else {Some(HSTRING::from(self.expanded_info))};

        if self.buttons.len() > 3 {
            if let Some(logger) = crate::logger::LOGGER.get() {
                logger.write_log_caller(LEVEL::WARN, &format!("[message_window] Unexpected behavior that may occur on non-Windows systems: the number of buttons exceeds 3, and the number of incoming buttons is {}.", self.buttons.len()), Location::caller());
            }
        };

        for item in self.buttons {
            match item {
                MessageWindowButton::ABORT      => c_buttons = c_buttons | TDCBF_ABORT_BUTTON,
                MessageWindowButton::CANCEL     => c_buttons = c_buttons | TDCBF_CANCEL_BUTTON,
                MessageWindowButton::CLOSE      => c_buttons = c_buttons | TDCBF_CLOSE_BUTTON,
                MessageWindowButton::CONTINUE   => c_buttons = c_buttons | TDCBF_CONTINUE_BUTTON,
                MessageWindowButton::HELP       => c_buttons = c_buttons | TDCBF_HELP_BUTTON,
                MessageWindowButton::IGNORE     => c_buttons = c_buttons | TDCBF_IGNORE_BUTTON,
                MessageWindowButton::NO         => c_buttons = c_buttons | TDCBF_NO_BUTTON,
                MessageWindowButton::OK         => c_buttons = c_buttons | TDCBF_OK_BUTTON,
                MessageWindowButton::RETRY      => c_buttons = c_buttons | TDCBF_RETRY_BUTTON,
                MessageWindowButton::TRYAGAIN   => c_buttons = c_buttons | TDCBF_TRYAGAIN_BUTTON,
                MessageWindowButton::YES        => c_buttons = c_buttons | TDCBF_YES_BUTTON,
                
                MessageWindowButton::Custom(int_var, content) => {
                    let text = HSTRING::from(content);

                    p_buttons.push(TASKDIALOG_BUTTON {
                        nButtonID: int_var,
                        pszButtonText: PCWSTR(text.as_ptr()),
                    });

                    button_texts.push(text);
                }
            }
        }

        let (icon, use_h_icon) = match self.icon {
            MessageWindowIcon::ERROR              => (TASKDIALOGCONFIG_0 { pszMainIcon: TD_ERROR_ICON }, false),
            MessageWindowIcon::INFO               => (TASKDIALOGCONFIG_0 { pszMainIcon: TD_INFORMATION_ICON }, false),
            MessageWindowIcon::SHIELD             => (TASKDIALOGCONFIG_0 { pszMainIcon: TD_SHIELD_ICON }, false),
            MessageWindowIcon::WARN               => (TASKDIALOGCONFIG_0 { pszMainIcon: TD_WARNING_ICON }, false),
            MessageWindowIcon::NULL               => (TASKDIALOGCONFIG_0 { pszMainIcon: PCWSTR::null()}, false),
            MessageWindowIcon::HICON(icon) => (TASKDIALOGCONFIG_0 { hMainIcon: icon }, true),
        };
        if use_h_icon { flags |= TDF_USE_HICON_MAIN; }; // 根据 Win32 文档，此处需要设置标志

        let config = TASKDIALOGCONFIG {
            cbSize: std::mem::size_of::<TASKDIALOGCONFIG>() as u32,
            hwndParent: HWND::default(),
            dwFlags: flags,

            dwCommonButtons: c_buttons,
            
            pszWindowTitle: PCWSTR(title.as_ptr()),
            pszMainInstruction: PCWSTR(instruction.as_ptr()),
            pszContent: PCWSTR(content.as_ptr()),

            cButtons: p_buttons.len() as u32,
            pButtons: p_buttons.as_ptr(),

            Anonymous1: icon,
            Anonymous2: TASKDIALOGCONFIG_1::default(),

            pszExpandedInformation: expanded_info.as_ref().map_or(PCWSTR::null(), |h| PCWSTR(h.as_ptr())),

            ..Default::default()

        };

        let mut button_pressed = 0i32;         
        let mut radio_pressed = 0i32;          
        let mut verification_checked = BOOL(0);
        
        unsafe {
            match TaskDialogIndirect(
                &config,                        
                Some(&mut button_pressed),      
                Some(&mut radio_pressed),       
                Some(&mut verification_checked),
            ) { // 后两个参数保留备用
                Err(error) => {return Err(MessageWindowError::WindowsSystem(error))},
                Ok(()) => {},
            }
        }

        let button_pressed_box = MESSAGEBOX_RESULT(button_pressed);

        return Ok(match button_pressed_box {
            IDABORT      => MessageWindowDialog::ABORT   ,
            IDCANCEL     => MessageWindowDialog::CANCEL  ,
            IDCLOSE      => MessageWindowDialog::CLOSE   ,
            IDCONTINUE   => MessageWindowDialog::CONTINUE,
            IDHELP       => MessageWindowDialog::HELP    ,
            IDIGNORE     => MessageWindowDialog::IGNORE  ,
            IDNO         => MessageWindowDialog::NO      ,
            IDOK         => MessageWindowDialog::OK      ,
            IDRETRY      => MessageWindowDialog::RETRY   ,
            IDTRYAGAIN   => MessageWindowDialog::TRYAGAIN,
            IDYES        => MessageWindowDialog::YES     ,
            _            => MessageWindowDialog::Custom(button_pressed),
        });
    }

    /// 非 Windows 平台调用 rfd 的对话框显示消息。
    ///
    /// 最多支持 3 个按钮，显示时将会拼接 instruction 与 content 等信息到正文。
    /// AIGC
    /// 尚未人工审核和 TEST
    #[cfg(not(target_os = "windows"))]
    pub fn show(self) -> Result<MessageWindowDialog, MessageWindowError> {
        /// AIGC
        fn join_description(instruction: &str, content: &str, expanded_info: &str) -> String {
            let parts: Vec<&str> = [instruction, content, expanded_info]
                .into_iter()
                .filter(|part| !part.is_empty())
                .collect();
            parts.join("\n\n")
        }

        /// rfd 中未定义文本的 UI 文本转换
        /// TODO: 国际化支持
        ///
        /// AIGC
        #[cfg(not(target_os = "windows"))]
        fn button_text(button: &MessageWindowButton) -> &str {
            match button {
                MessageWindowButton::ABORT      => "Abort",
                MessageWindowButton::CANCEL     => "Cancel",
                MessageWindowButton::CLOSE      => "Close",
                MessageWindowButton::CONTINUE   => "Continue",
                MessageWindowButton::HELP       => "Help",
                MessageWindowButton::IGNORE     => "Ignore",
                MessageWindowButton::NO         => "No",
                MessageWindowButton::OK         => "OK",
                MessageWindowButton::RETRY      => "Retry",
                MessageWindowButton::TRYAGAIN   => "Try Again",
                MessageWindowButton::YES        => "Yes",
                MessageWindowButton::Custom(_, text) => text.as_str(),
            }
        }

        /// 把按钮列表翻译成 rfd 的按钮组合。
        /// 
        /// # Errors
        /// 受 rfd 的 3 个按钮限制，超出时抛出 [`MessageWindowError::TooManyButtons`]。
        /// AIGC
        #[cfg(not(target_os = "windows"))]
        fn to_rfd_buttons(buttons: &[MessageWindowButton]) -> Result<MessageButtons, MessageWindowError> {
            match buttons {
                [MessageWindowButton::OK] => Ok(MessageButtons::Ok),
                [MessageWindowButton::OK, MessageWindowButton::CANCEL] => Ok(MessageButtons::OkCancel),
                [MessageWindowButton::YES, MessageWindowButton::NO] => Ok(MessageButtons::YesNo),
                [MessageWindowButton::YES, MessageWindowButton::NO, MessageWindowButton::CANCEL] => Ok(MessageButtons::YesNoCancel),
            
                // 啥都没有，依照惯例给个 OK
                [] => Ok(MessageButtons::Ok),
                [button] => Ok(MessageButtons::OkCustom(button_text(button).to_string())),
                [first, second] => Ok(MessageButtons::OkCancelCustom(
                    button_text(first).to_string(),
                    button_text(second).to_string(),
                )),
                [first, second, third] => Ok(MessageButtons::YesNoCancelCustom(
                    button_text(first).to_string(),
                    button_text(second).to_string(),
                    button_text(third).to_string(),
                )),
            
                _ => Err(MessageWindowError::TooManyButtons(buttons.len())),
            }
        }

        /// 把 rfd 的返回值翻译回本模块的返回值。
        ///
        /// 如果遇到非正常（如可能的窗口关闭导致的无正常返回值）时，返回 `MessageWindowDialog::NULL`
        /// AIGC
        #[cfg(not(target_os = "windows"))]
        fn to_dialog(result: MessageDialogResult, buttons: &[MessageWindowButton]) -> MessageWindowDialog {
            match result {
                MessageDialogResult::Ok => buttons
                    .first()
                    .map_or(MessageWindowDialog::OK, dialog_for_button),
            
                MessageDialogResult::Yes    => MessageWindowDialog::YES,
                MessageDialogResult::No     => MessageWindowDialog::NO,
                MessageDialogResult::Cancel => MessageWindowDialog::CANCEL,
                MessageDialogResult::Custom(text) => buttons
                    .iter()
                    .find(|button| button_text(button) == text.as_str())
                    .map(dialog_for_button)
                    .unwrap_or(MessageWindowDialog::NULL),
            }
        }

        /// 单个按钮对应的返回值。
        /// AIGC
        #[cfg(not(target_os = "windows"))]
        fn dialog_for_button(button: &MessageWindowButton) -> MessageWindowDialog {
            match button {
                MessageWindowButton::ABORT      => MessageWindowDialog::ABORT,
                MessageWindowButton::CANCEL     => MessageWindowDialog::CANCEL,
                MessageWindowButton::CLOSE      => MessageWindowDialog::CLOSE,
                MessageWindowButton::CONTINUE   => MessageWindowDialog::CONTINUE,
                MessageWindowButton::HELP       => MessageWindowDialog::HELP,
                MessageWindowButton::IGNORE     => MessageWindowDialog::IGNORE,
                MessageWindowButton::NO         => MessageWindowDialog::NO,
                MessageWindowButton::OK         => MessageWindowDialog::OK,
                MessageWindowButton::RETRY      => MessageWindowDialog::RETRY,
                MessageWindowButton::TRYAGAIN   => MessageWindowDialog::TRYAGAIN,
                MessageWindowButton::YES        => MessageWindowDialog::YES,
                MessageWindowButton::Custom(id, _) => MessageWindowDialog::Custom(*id),
                _ => MessageWindowDialog::NULL,
            }
        }

        /// 降级函数
        /// NULL | SHIELD | INFO => INFO
        /// AIGC
        #[cfg(not(target_os = "windows"))]
        fn to_rfd_level(icon: &MessageWindowIcon) -> MessageLevel {
            match icon {
                MessageWindowIcon::ERROR  => MessageLevel::Error,
                MessageWindowIcon::WARN   => MessageLevel::Warning,
                MessageWindowIcon::NULL | MessageWindowIcon::SHIELD | MessageWindowIcon::INFO => {
                    MessageLevel::Info
                }
            }
        }

        ////////////////////////////////////////////
        // -------------------------------------- //
        ////////////////////////////////////////////

        let rfd_buttons = to_rfd_buttons(&self.buttons)?;

        let result = MessageDialog::new()
            .set_title(self.title)
            .set_description(join_description(
                &self.instruction,
                &self.content,
                &self.expanded_info,
            ))
            .set_level(to_rfd_level(&self.icon))
            .set_buttons(rfd_buttons)
            .show();

        Ok(to_dialog(result, &self.buttons))
    }
    
}
