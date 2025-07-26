use dioxus::prelude::*;

#[component]
pub fn SimpleForm(
    children: Element,
    onsubmit: EventHandler<FormEvent>,
    #[props(default = "off".to_string())] autocomplete: String,
) -> Element {
    rsx! {
        form {
            class: "space-y-6",
            autocomplete: autocomplete,
            onsubmit: move |evt| {
                evt.prevent_default();
                onsubmit.call(evt);
            },
            div {
                class: "mt-10 space-y-8 bg-white dark:bg-transparent dark:text-gray-200",
                {children}
            }
        }
    }
}

#[component]
pub fn Input(
    field_name: String,
    value: String,
    placeholder: String,
    #[props(default = "text".to_string())] input_type: String,
    #[props(default = false)] required: bool,
    #[props(default = None)] maxlength: Option<String>,
    #[props(default = None)] minlength: Option<String>,
    #[props(default = None)] max: Option<String>,
    #[props(default = None)] min: Option<String>,
    #[props(default = None)] pattern: Option<String>,
    oninput: EventHandler<FormEvent>,
) -> Element {
    let input_classes = "input";

    rsx! {
        div {
            input {
                r#type: input_type,
                name: field_name.clone(),
                id: field_name,
                class: input_classes,
                placeholder: placeholder,
                value: value,
                required: required,
                maxlength: maxlength.as_deref(),
                minlength: minlength.as_deref(),
                min: min.as_deref(),
                max: max.as_deref(),
                pattern: pattern.as_deref(),
                oninput: move |evt| oninput.call(evt)
            }
        }
    }
}

#[component]
pub fn Button(
    children: Element,
    #[props(default = "button".to_string())] button_type: String,
    #[props(default = "".to_string())] class: String,
) -> Element {
    let base_classes = "phx-submit-loading:opacity-75 rounded-lg bg-zinc-900 hover:bg-zinc-700 py-2 px-3 text-sm font-semibold leading-6 text-white active:text-white/80";
    let combined_classes = if class.is_empty() {
        base_classes.to_string()
    } else {
        format!("{} {}", base_classes, class)
    };

    rsx! {
        button {
            r#type: button_type,
            class: combined_classes,
            {children}
        }
    }
}

#[component]
pub fn FormActions(children: Element) -> Element {
    rsx! {
        div {
            class: "mt-2 flex items-center justify-between gap-6",
            {children}
        }
    }
}

#[component]
pub fn Label(
    r#for: String,
    children: Element,
    #[props(default = "".to_string())] class: String,
) -> Element {
    let base_classes = "block text-sm font-semibold leading-6 text-zinc-800 dark:text-zinc-100";
    let combined_classes = if class.is_empty() {
        base_classes.to_string()
    } else {
        format!("{} {}", base_classes, class)
    };

    rsx! {
        label {
            r#for: r#for,
            class: combined_classes,
            {children}
        }
    }
}

#[component]
pub fn ErrorMessage(
    message: Option<String>,
    #[props(default = "".to_string())] class: String,
) -> Element {
    let base_classes = "mt-3 flex gap-3 text-sm leading-6 text-rose-600 phx-no-feedback:hidden";
    let combined_classes = if class.is_empty() {
        base_classes.to_string()
    } else {
        format!("{} {}", base_classes, class)
    };

    if let Some(msg) = message {
        rsx! {
            p {
                class: combined_classes,
                span { class: "h-4 w-4", "⚠" }
                "{msg}"
            }
        }
    } else {
        rsx! { div {} }
    }
}
