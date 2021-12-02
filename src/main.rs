use std::cell::RefCell;
use std::rc::Rc;
use std::thread;
use std::time::Duration;

use gtk4 as gtk;
use gtk::gio::{Menu, MenuItem, SimpleAction};

use gtk::{AboutDialog, Align, HeaderBar, Label, MenuButton};
use gtk::prelude::*;
use gtk::{Application, ApplicationWindow, Button, Box as GtkBox, Image};

use glib::{Continue, MainContext, PRIORITY_DEFAULT, Receiver, Sender, clone};

use preferences::{Preferences, PreferencesWindowWrapper};

mod preferences;

fn main() {
    // gst::init().map_err(|_| "Cannot initialize Gstreamer").unwrap();
    gtk::init().expect("无法初始化 GTK4");
    adw::init();
    let app = Application::builder()
        .application_id("org.coco24.rovhost-gtk")
        .build();
    app.connect_activate(|app| {
        let main_window = MainWindowWrapper::new(app);
        main_window.window().present();
    });
    app.run();
}

enum MainWindowAction {
    StartRecord,
    StopRecord,
}

enum MainWindowUIAction {
    RecordStarted,
    RecordStopped,
}

struct VideoRecorder {
    recording: bool
}

struct RobotController {
    
}

impl VideoRecorder {
    fn start_record(&mut self) {
        self.recording = true;
    }

    fn stop_record(&mut self) {
        self.recording = false;
    }
    
    fn recording(&self) -> bool {
        self.recording
    }
}

struct MainWindowWrapper {
    window: ApplicationWindow,
    ui_sender: Sender<MainWindowUIAction>,
    video_recorder: Rc<RefCell<VideoRecorder>>,
    robot_controller: Rc<RefCell<RobotController>>,
    preferences: Rc<RefCell<Preferences>>,
    // 1. 使用 ~Rc~ 的目的： \\
    //    把一个对象转移到闭包里要得到它的所有权，如果直接使用 ~RefCell~ 则要求声明为静态生命周期。
    // 2. 使用 ~RefCell~ 的目的： \\
    //    得到可变引用用于开始、停止录制，如果直接使用 ~Rc~ 则无法获得可变引用。
}

impl MainWindowWrapper {
    pub fn new(app: &Application) -> MainWindowWrapper {
        let window = ApplicationWindow::builder()
            .application(app)
            .title("水下机器人上位机")
            .build();
        let (action_sender, action_receiver) = MainContext::channel(PRIORITY_DEFAULT);
        let (ui_sender, ui_receiver) = MainContext::channel(PRIORITY_DEFAULT);
        let video_recorder = VideoRecorder {
            recording: false
        };
        let robot_controller = RobotController {
            
        };
        let preferences = Preferences::default();
        let wrapper = MainWindowWrapper {
            window,
            ui_sender,
            video_recorder: Rc::new(RefCell::new(video_recorder)),
            robot_controller: Rc::new(RefCell::new(robot_controller)),
            preferences: Rc::new(RefCell::new(preferences))
        };
        
        wrapper.build_ui(action_sender, ui_receiver);
        wrapper.handle_actions(action_receiver);
        wrapper
    }
    
    pub fn window(&self) -> &ApplicationWindow {
        &self.window
    }

    fn handle_actions(&self, receiver: Receiver<MainWindowAction>) {
        let ui_sender = self.ui_sender.clone();
        let video_recorder = self.video_recorder.clone();
        receiver.attach(None, move |action| {
            match action {
                MainWindowAction::StartRecord => {
                    video_recorder.borrow_mut().start_record();
                    thread::spawn(clone!(@strong ui_sender => move || {
                        thread::sleep(Duration::from_secs(1));
                        ui_sender.send(MainWindowUIAction::RecordStarted);
                    }));
                },
                MainWindowAction::StopRecord => {
                    video_recorder.borrow_mut().stop_record();
                    thread::spawn(clone!(@strong ui_sender => move || {
                        thread::sleep(Duration::from_secs(1));
                        ui_sender.send(MainWindowUIAction::RecordStopped);
                    }));
                }
            }
            Continue(true)
        });
    }

    fn build_ui(&self, sender: Sender<MainWindowAction>, receiver: Receiver<MainWindowUIAction>) {
        let mut ui_action_handlers = Vec::new();
        let head_bar = HeaderBar::builder().build();
        
        let menu = {
            let menu = Menu::new();
            let menu_item_preference = MenuItem::new(Some("首选项"), None);
            let menu_item_keybindings = MenuItem::new(Some("键盘快捷键"), None);
            let menu_item_about = MenuItem::new(Some("关于"), None);
            menu_item_preference.set_action_and_target_value(Some("app.preference"), None);
            menu_item_keybindings.set_action_and_target_value(Some("app.keybindings"), None);
            menu_item_about.set_action_and_target_value(Some("app.about"), None);
            menu.append_item(&menu_item_preference);
            menu.append_item(&menu_item_keybindings);
            menu.append_item(&menu_item_about);
            menu
        };
        
        let record_button = {
            let button = Button::builder() .halign(Align::Center).build();
            let icon = Image::builder()
                .valign(Align::Center)
                .icon_name("media-record-symbolic")
                .build();
            let label = Label::builder()
                .label("录制")
                .build();
            let gtkbox = GtkBox::builder()
                .spacing(6)
                .build();
            gtkbox.append(&icon);
            gtkbox.append(&label);
            button.set_child(Some(&gtkbox));
            ui_action_handlers.push(clone!(@strong label, @strong icon, @strong button => move |ui_action: MainWindowUIAction| {
                match ui_action {
                    MainWindowUIAction::RecordStarted => {
                        label.set_text("停止");
                        icon.set_icon_name(Some("media-playback-stop-symbolic"));
                        button.add_css_class("destructive-action");
                        button.set_sensitive(true);
                        None
                    }
                    MainWindowUIAction::RecordStopped => {
                        label.set_text("录制");
                        icon.set_icon_name(Some("media-record-symbolic"));
                        button.remove_css_class("destructive-action");
                        button.set_sensitive(true);
                        None
                    }
                    _ => Some(ui_action)
                }
            }));
            let video_recorder = self.video_recorder.clone();
            button.connect_clicked(move |button| {
                button.set_sensitive(false);
                if video_recorder.borrow().recording() {
                    sender.send(MainWindowAction::StopRecord);
                } else {
                    sender.send(MainWindowAction::StartRecord);
                }
            });
            button
        };
        head_bar.pack_start(&record_button);
    
        let menu_button = MenuButton::builder()
            .valign(Align::Center)
            .focus_on_click(false)
            .menu_model(&menu)
            .icon_name("open-menu-symbolic")
            .build();
        head_bar.pack_end(&menu_button);
    
        // let settings_button = Button::builder()
        //     .icon_name("emblem-system-symbolic")
        //     .build();
        // settings_button.add_css_class("circular");
        // settings_button.set_action_name(Some("app.preference"));
        // head_bar.pack_end(&settings_button);
        
        self.window.set_titlebar(Some(&head_bar));
        
        let application = self.window.application().unwrap();

        let action_keybindings = SimpleAction::new("keybindings", None);
        action_keybindings.connect_activate(move |_, _| {
            
        });
        application.add_action(&action_keybindings);

        let action_about = SimpleAction::new("about", None);

        let window = self.window.clone();
        action_about.connect_activate(move |_, _| {
            let about_window = AboutDialog::builder()
                .transient_for(&window)
                .destroy_with_parent(true)
                .can_focus(false)
                .modal(true)
                .authors(vec![String::from("黄博宏")])
                .program_name("水下机器人上位机")
                .copyright("© 2021 集美大学水下智能创新实验室")
                .comments("跨平台的校园水下机器人上位机程序")
                .logo_icon_name("applications-games")
                .version("0.0.1")
                .build();
            about_window.show();
        });
        application.add_action(&action_about);
        receiver.attach(None, move |ui_action| {
            ui_action_handlers.iter().fold(Some(ui_action), |acc, it| {
                match acc {
                    Some(action) => it(action),
                    None => None,
                }
            });
            Continue(true)
        });

        let action_preference = SimpleAction::new("preference", None);
        let window = self.window.clone();
        let preference = self.preferences.clone();
        action_preference.connect_activate(move |_, _| {
            let preference_window = PreferencesWindowWrapper::new(&window, preference.clone()); // 由于 ~clone()~ 方法是对引用使用的，因此可以满足 ~Fn~ 的要求（即使传入了捕获变量的所有权，也只能使用捕获变量的引用）
            preference_window.window().present();
        });
        application.add_action(&action_preference);
    }
}

