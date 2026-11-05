#[cfg(target_os = "windows")]
use windows::Win32::UI::Controls::{TASKDIALOG_COMMON_BUTTON_FLAGS, TDCBF_CANCEL_BUTTON, TDCBF_CLOSE_BUTTON, TDCBF_NO_BUTTON, TDCBF_RETRY_BUTTON, TDCBF_YES_BUTTON};
#[cfg(target_os = "windows")]
use windows::core::{HRESULT, HSTRING, PCWSTR};
#[cfg(target_os = "windows")]
use windows::Win32::Foundation::{HWND, LPARAM, S_FALSE, S_OK, WPARAM};
#[cfg(target_os = "windows")]
use windows::core::BOOL;
#[cfg(target_os = "windows")]
use windows::Win32::UI::Controls::{
    TaskDialogIndirect, TASKDIALOGCONFIG, TASKDIALOGCONFIG_0, TASKDIALOGCONFIG_1,
    TASKDIALOG_BUTTON, TASKDIALOG_NOTIFICATIONS, TDCBF_OK_BUTTON, TDF_ALLOW_DIALOG_CANCELLATION,
    TD_INFORMATION_ICON, TDN_BUTTON_CLICKED, TDN_CREATED, TDN_DESTROYED, TDN_DIALOG_CONSTRUCTED,
    TDN_EXPANDO_BUTTON_CLICKED, TDN_HELP, TDN_HYPERLINK_CLICKED, TDN_NAVIGATED,
    TDN_RADIO_BUTTON_CLICKED, TDN_TIMER, TDN_VERIFICATION_CLICKED,
};
#[cfg(target_os = "windows")]
use windows::Win32::UI::WindowsAndMessaging::HICON;

#[cfg(not(target_os = "windows"))]
use rfd::{MessageButtons, MessageDialog, MessageDialogResult, MessageLevel};

use crate::logger::LEVEL;
#[cfg(target_os = "windows")]
use crate::logger::{LOGGER, LoggerGlobal};
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

/// TaskDialog 产生的通知。
///
/// 通过 [`MessageWindow::show_with_callback`] 注册的回调接收
/// AIGC
#[cfg(target_os = "windows")]
#[allow(dead_code)]
pub enum MessageWindowEvent {
    /// 窗口已创建。
    Created,
    /// 对话框已跳转到新页面。
    Navigated,
    /// 按钮被点击，参数为按钮 ID（自定义按钮即传入 [`MessageWindowButton::Custom`] 的编号）。
    ButtonClicked(i32),
    /// 超链接被点击，参数为链接地址。
    HyperlinkClicked(String),
    /// 定时器触发，参数为距上次触发经过的毫秒数。
    Timer(u32),
    /// 窗口已销毁。
    Destroyed,
    /// 单选按钮被点击，参数为按钮 ID。
    RadioButtonClicked(i32),
    /// 对话框内部控件构造完成，此时才可以向窗口发送 `TDM_*` 消息。
    DialogConstructed,
    /// 验证框状态改变，参数为当前是否勾选。
    VerificationClicked(bool),
    /// 帮助按钮或 F1 被按下。
    Help,
    /// 详细信息区域被展开或折叠，参数为当前是否展开。
    ExpandoButtonClicked(bool),
    /// 本模块尚未识别的通知，参数为 `TDN_*` 的原始值。
    Unknown(i32),
}

#[derive(Debug)]
pub enum MessageWindowError {
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
    expanded_info: String,
    expanded_info_button_text: String,
    collapsed_info_button_text: String,
    use_hyperlink: bool,
}

impl std::fmt::Display for MessageWindowError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            #[cfg(target_os = "windows")]
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

/// 通过 `lpCallbackData` 交给 Win32 的回调载体。
///
/// TaskDialog 在调用线程内同步触发回调，`show_inner` 的栈帧在整个
/// `TaskDialogIndirect` 调用期间存活，因此这里不需要 `'static` 约束；
/// 也不能把它交给其他线程使用。
/// AIGC
#[cfg(target_os = "windows")]
struct CallbackData<'a> {
    callback: Option<&'a mut dyn FnMut(HWND, MessageWindowEvent) -> bool>,
}

/// TaskDialog 的 C 回调入口。
///
/// `data` 为 `show_inner` 中 [`CallbackData`] 的地址；回调为空时表示调用方未注册
/// 自定义回调，此时由本模块处理超链接点击。
/// AIGC
#[cfg(target_os = "windows")]
unsafe extern "system" fn task_dialog_callback(
    hwnd: HWND,
    notification: TASKDIALOG_NOTIFICATIONS,
    wparam: WPARAM,
    lparam: LPARAM,
    data: isize,
) -> HRESULT {
    // SAFETY: data 由 show_inner 传入，指向在 TaskDialogIndirect 返回前一直存活的
    // CallbackData；TaskDialog 的回调在调用线程内同步执行，不会跨线程。
    let callback = unsafe { &mut *(data as *mut CallbackData) }.callback.as_mut();
    let Some(callback) = callback else {
        if notification == TDN_HYPERLINK_CLICKED {
            // SAFETY: 依据 TDN_HYPERLINK_CLICKED 的约定，wParam 必须为 0，lParam
            // 指向以 0 结尾的 URL 字符串，该字符串在本次回调期间有效。
            open_hyperlink(PCWSTR::from_raw(lparam.0 as *const u16));
        }
        return S_OK;
    };

    // SAFETY: callback 非空且指向 show_inner 中存活的闭包；同一窗口的回调不会重入。
    let callback = &mut *callback;
    if callback(hwnd, event_for_notification(notification, wparam, lparam)) {
        S_FALSE
    } else {
        S_OK
    }
}

/// 用系统默认程序打开超链接，失败时写入日志。
/// AIGC
#[cfg(target_os = "windows")]
fn open_hyperlink(url: PCWSTR) {
    if url.is_null() {
        return;
    }
    // SAFETY: 调用方保证 url 指向本次回调期间有效的、以 0 结尾的字符串。
    let url = unsafe { url.to_string() }.unwrap_or_default();
    LOGGER.debug(&format!("Open URL: {url}"));
    if url.is_empty() {
        return;
    }
    if let Err(error) = crate::app::open_url::open(&url) {
        if let Some(logger) = crate::logger::LOGGER.get() {
            logger.write_log_caller(
                LEVEL::WARN,
                &format!("[message_window] 无法打开超链接：{error}"),
                Location::caller(),
            );
        }
    }
}

/// 把 Win32 通知翻译成模块事件。
/// AIGC
#[cfg(target_os = "windows")]
fn event_for_notification(
    notification: TASKDIALOG_NOTIFICATIONS,
    wparam: WPARAM,
    lparam: LPARAM,
) -> MessageWindowEvent {
    match notification {
        TDN_CREATED => MessageWindowEvent::Created,
        TDN_NAVIGATED => MessageWindowEvent::Navigated,
        TDN_BUTTON_CLICKED => MessageWindowEvent::ButtonClicked(wparam.0 as i32),
        TDN_HYPERLINK_CLICKED => MessageWindowEvent::HyperlinkClicked({
            // SAFETY: 同 task_dialog_callback 中的说明；lParam 为空时按空链接处理。
            let url = PCWSTR::from_raw(lparam.0 as *const u16);
            if url.is_null() {
                String::new()
            } else {
                unsafe { url.to_string() }.unwrap_or_default()
            }
        }),
        TDN_TIMER => MessageWindowEvent::Timer(wparam.0 as u32),
        TDN_DESTROYED => MessageWindowEvent::Destroyed,
        TDN_RADIO_BUTTON_CLICKED => MessageWindowEvent::RadioButtonClicked(wparam.0 as i32),
        TDN_DIALOG_CONSTRUCTED => MessageWindowEvent::DialogConstructed,
        TDN_VERIFICATION_CLICKED => MessageWindowEvent::VerificationClicked(wparam.0 != 0),
        TDN_HELP => MessageWindowEvent::Help,
        TDN_EXPANDO_BUTTON_CLICKED => MessageWindowEvent::ExpandoButtonClicked(wparam.0 != 0),
        _ => MessageWindowEvent::Unknown(notification.0),
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
            expanded_info_button_text: String::new(),
            collapsed_info_button_text: String::new(),
            use_hyperlink: false,

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
    /// 可以使用 `set_expanded_info_button` 修改按钮的文本
    pub fn set_expanded_info(mut self, expanded_info: &str) -> Self {
        self.expanded_info = expanded_info.to_string();
        self
    }

    /// 用于显示详细信息的按钮的文本
    /// 仅 Windows 原生支持，非 Windows 会加 `expanded_text` 到详细信息的前面：`XXX: \n`
    pub fn set_expanded_info_button(mut self, expanded_text: &str, collapsed_text: &str) -> Self {
        self.expanded_info_button_text = expanded_text.to_string();
        self.collapsed_info_button_text = collapsed_text.to_string();
        self
    }

    /// TDF_ENABLE_HYPERLINKS 参数的指定，该参数用于允许在窗口中显示可点击的链接
    /// 仅 Windows 原生支持，非 Windows 会被丢弃
    /// - **WARNING**: 在非 Windows 平台使用这个时候，模块不会自动处理文本中的 A 标签（即原样显示），请注意分流
    /// - **WARNING**: 使用来自不安全源的内容时启用超链接可能会导致安全漏洞。
    pub fn use_hyperlink(mut self, enable: bool) -> Self {
        self.use_hyperlink = enable;
        self
    }

    /// 尝试在 Windows 上使用 TaskDialog 显示窗口。
    /// 如果需要自行处理回调，请改用 [`Self::show_with_callback`]。
    #[cfg(target_os = "windows")]
    #[track_caller]
    pub fn show(self) -> Result<MessageWindowDialog, MessageWindowError> {
        self.show_inner(None)
    }

    /// 显示窗口，并注册自定义通知回调。
    /// - 回调在窗口所属线程内同步执行，可以直接借用外部状态。返回 `true` 表示阻止默认处理（`S_FALSE`）。
    /// - 注册回调后模块不再自动打开超链接，点击事件以 [`MessageWindowEvent::HyperlinkClicked`] 交给回调。
    #[cfg(target_os = "windows")]
    #[track_caller]
    pub fn show_with_callback<F>(
        self,
        callback: F,
    ) -> Result<MessageWindowDialog, MessageWindowError>
    where
        F: FnMut(HWND, MessageWindowEvent) -> bool,
    {
        let mut callback = callback;
        self.show_inner(Some(&mut callback))
    }

    /// Windows 上的实现；`callback` 为空时由模块处理超链接。
    #[cfg(target_os = "windows")]
    #[track_caller]
    fn show_inner(
        self,
        callback: Option<&mut dyn FnMut(HWND, MessageWindowEvent) -> bool>,
    ) -> Result<MessageWindowDialog, MessageWindowError> {
        use windows::Win32::UI::Controls::{TD_ERROR_ICON, TD_SHIELD_ICON, TD_WARNING_ICON};
        use windows::Win32::UI::Controls::{TDCBF_ABORT_BUTTON, TDF_USE_HICON_MAIN, TDCBF_CONTINUE_BUTTON, TDCBF_HELP_BUTTON, TDCBF_IGNORE_BUTTON, TDCBF_TRYAGAIN_BUTTON, TDF_ENABLE_HYPERLINKS};
        use windows::Win32::UI::WindowsAndMessaging::{MESSAGEBOX_RESULT, IDABORT, IDCANCEL, IDNO, IDCLOSE, IDCONTINUE, IDHELP, IDIGNORE, IDOK, IDRETRY, IDTRYAGAIN, IDYES};

        let mut p_buttons = vec![];
        let mut c_buttons = TASKDIALOG_COMMON_BUTTON_FLAGS(0);
        let mut button_texts: Vec<HSTRING> = Vec::new(); // 文本指针指向，目的是为了防止提前释放内存导致的非预期行为
        let mut flags = TDF_ALLOW_DIALOG_CANCELLATION;

        let content = HSTRING::from(self.content);
        let title = HSTRING::from(self.title);
        let instruction = HSTRING::from(self.instruction);
        let expanded_info = if self.expanded_info.is_empty() {None} else {Some(HSTRING::from(self.expanded_info))};
        let expanded_control_text = HSTRING::from(self.expanded_info_button_text);
        let collapsed_control_text = HSTRING::from(self.collapsed_info_button_text);

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
        if self.use_hyperlink { flags |= TDF_ENABLE_HYPERLINKS };

        let mut callback_data = CallbackData { callback };

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
            pszExpandedControlText: PCWSTR(expanded_control_text.as_ptr()),
            pszCollapsedControlText: PCWSTR(collapsed_control_text.as_ptr()),

            pfCallback: Some(task_dialog_callback),
            lpCallbackData: &mut callback_data as *mut CallbackData as isize,

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
        fn join_description(instruction: &str, content: &str, expanded_info_button_text: &str, expanded_info: &str) -> String {
            let parts: Vec<&str> = [instruction, content, expanded_info_button_text, expanded_info]
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
                &self.expanded_info_button_text,
                &self.expanded_info,
            ))
            .set_level(to_rfd_level(&self.icon))
            .set_buttons(rfd_buttons)
            .show();

        Ok(to_dialog(result, &self.buttons))
    }
    
}
