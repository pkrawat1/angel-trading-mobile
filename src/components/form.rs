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
    #[props(default = false)] disabled: bool,
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
            disabled: disabled,
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
    #[props(default = false)] dismissible: bool,
) -> Element {
    let base_classes = "alert alert-error mt-3";
    let combined_classes = if class.is_empty() {
        base_classes.to_string()
    } else {
        format!("{} {}", base_classes, class)
    };

    if let Some(msg) = message {
        rsx! {
            div {
                role: "alert",
                class: combined_classes,
                // Error icon SVG following DaisyUI pattern
                svg {
                    xmlns: "http://www.w3.org/2000/svg",
                    class: "h-6 w-6 shrink-0 stroke-current",
                    fill: "none",
                    view_box: "0 0 24 24",
                    path {
                        stroke_linecap: "round",
                        stroke_linejoin: "round",
                        stroke_width: "2",
                        d: "M10 14l2-2m0 0l2-2m-2 2l-2-2m2 2l2 2m7-2a9 9 0 11-18 0 9 9 0 0118 0z"
                    }
                }
                span { "{msg}" }
                // Optional close button for dismissible errors
                if dismissible {
                    button {
                        class: "btn btn-sm btn-ghost",
                        "aria-label": "Close",
                        svg {
                            xmlns: "http://www.w3.org/2000/svg",
                            class: "h-4 w-4",
                            fill: "none",
                            view_box: "0 0 24 24",
                            stroke: "currentColor",
                            path {
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                stroke_width: "2",
                                d: "M6 18L18 6M6 6l12 12"
                            }
                        }
                    }
                }
            }
        }
    } else {
        rsx! { div {} }
    }
}

#[component]
pub fn SuccessMessage(
    message: Option<String>,
    #[props(default = "".to_string())] class: String,
    #[props(default = false)] dismissible: bool,
) -> Element {
    let base_classes = "alert alert-success mt-3";
    let combined_classes = if class.is_empty() {
        base_classes.to_string()
    } else {
        format!("{} {}", base_classes, class)
    };

    if let Some(msg) = message {
        rsx! {
            div {
                role: "alert",
                class: combined_classes,
                // Success icon SVG following DaisyUI pattern
                svg {
                    xmlns: "http://www.w3.org/2000/svg",
                    class: "h-6 w-6 shrink-0 stroke-current",
                    fill: "none",
                    view_box: "0 0 24 24",
                    path {
                        stroke_linecap: "round",
                        stroke_linejoin: "round",
                        stroke_width: "2",
                        d: "M9 12l2 2 4-4m6 2a9 9 0 11-18 0 9 9 0 0118 0z"
                    }
                }
                span { "{msg}" }
                if dismissible {
                    button {
                        class: "btn btn-sm btn-ghost",
                        "aria-label": "Close",
                        svg {
                            xmlns: "http://www.w3.org/2000/svg",
                            class: "h-4 w-4",
                            fill: "none",
                            view_box: "0 0 24 24",
                            stroke: "currentColor",
                            path {
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                stroke_width: "2",
                                d: "M6 18L18 6M6 6l12 12"
                            }
                        }
                    }
                }
            }
        }
    } else {
        rsx! { div {} }
    }
}

#[component]
pub fn WarningMessage(
    message: Option<String>,
    #[props(default = "".to_string())] class: String,
    #[props(default = false)] dismissible: bool,
) -> Element {
    let base_classes = "alert alert-warning mt-3";
    let combined_classes = if class.is_empty() {
        base_classes.to_string()
    } else {
        format!("{} {}", base_classes, class)
    };

    if let Some(msg) = message {
        rsx! {
            div {
                role: "alert",
                class: combined_classes,
                // Warning icon SVG following DaisyUI pattern
                svg {
                    xmlns: "http://www.w3.org/2000/svg",
                    class: "h-6 w-6 shrink-0 stroke-current",
                    fill: "none",
                    view_box: "0 0 24 24",
                    path {
                        stroke_linecap: "round",
                        stroke_linejoin: "round",
                        stroke_width: "2",
                        d: "M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z"
                    }
                }
                span { "{msg}" }
                if dismissible {
                    button {
                        class: "btn btn-sm btn-ghost",
                        "aria-label": "Close",
                        svg {
                            xmlns: "http://www.w3.org/2000/svg",
                            class: "h-4 w-4",
                            fill: "none",
                            view_box: "0 0 24 24",
                            stroke: "currentColor",
                            path {
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                stroke_width: "2",
                                d: "M6 18L18 6M6 6l12 12"
                            }
                        }
                    }
                }
            }
        }
    } else {
        rsx! { div {} }
    }
}

#[component]
pub fn InfoMessage(
    message: Option<String>,
    #[props(default = "".to_string())] class: String,
    #[props(default = false)] dismissible: bool,
) -> Element {
    let base_classes = "alert alert-info mt-3";
    let combined_classes = if class.is_empty() {
        base_classes.to_string()
    } else {
        format!("{} {}", base_classes, class)
    };

    if let Some(msg) = message {
        rsx! {
            div {
                role: "alert",
                class: combined_classes,
                // Info icon SVG following DaisyUI pattern
                svg {
                    xmlns: "http://www.w3.org/2000/svg",
                    class: "h-6 w-6 shrink-0 stroke-current",
                    fill: "none",
                    view_box: "0 0 24 24",
                    path {
                        stroke_linecap: "round",
                        stroke_linejoin: "round",
                        stroke_width: "2",
                        d: "M13 16h-1v-4h-1m1-4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z"
                    }
                }
                span { "{msg}" }
                if dismissible {
                    button {
                        class: "btn btn-sm btn-ghost",
                        "aria-label": "Close",
                        svg {
                            xmlns: "http://www.w3.org/2000/svg",
                            class: "h-4 w-4",
                            fill: "none",
                            view_box: "0 0 24 24",
                            stroke: "currentColor",
                            path {
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                stroke_width: "2",
                                d: "M6 18L18 6M6 6l12 12"
                            }
                        }
                    }
                }
            }
        }
    } else {
        rsx! { div {} }
    }
}
