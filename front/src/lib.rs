use anyhow;
use wasm_bindgen::prelude::*;
use yew::prelude::*;
use yew::services::fetch::{FetchTask, Request, Response};

struct Model {
    link: ComponentLink<Self>,
    message: Option<String>,
    error: Option<String>,
    fetch_task: Option<FetchTask>,
    id: String,
    target: String,
}

enum Msg {
    Create(),
    ReceiveResponse(Result<String, anyhow::Error>),
    UpdateId(String),
    UpdateTarget(String),
}

impl Model {
    fn view_form(&self) -> Html {
        let oninput_id = self.link.callback(|e: InputData| Msg::UpdateId(e.value));

        let oninput_target = self
            .link
            .callback(|e: InputData| Msg::UpdateTarget(e.value));

        html! {
            <>
                <h1>{ "Short URL" }</h1>
                <input type="text" placeholder="shortened_url" oninput=oninput_id value=self.id.clone() /><br />
                <input type="text" placeholder="https://linkedin.com/in/tsauvajon/" oninput=oninput_target value=self.target.clone() />
                <button onclick=self.link.callback(|_| Msg::Create())>
                    { "Shorten URL" }
                </button>
            </>
        }
    }

    fn view_message(&self) -> Html {
        match self.message.clone() {
            Some(msg) => html! { <p>{ msg }</p> },
            None => html! {},
        }
    }

    fn view_error(&self) -> Html {
        match self.error.clone() {
            Some(err) => html! { <p>{ err }</p> },
            None => html! {},
        }
    }

    fn view_fetching_task(&self) -> Html {
        match self.fetch_task {
            Some(_) => html! { <p>{ "Fetching data..." }</p> },
            None => html! {},
        }
    }
}

impl Component for Model {
    type Message = Msg;
    type Properties = ();
    fn create(_: Self::Properties, link: ComponentLink<Self>) -> Self {
        Self {
            link,
            message: None,
            error: None,
            fetch_task: None,
            id: "".to_string(),
            target: "".to_string(),
        }
    }

    fn update(&mut self, msg: Self::Message) -> ShouldRender {
        match msg {
            Msg::Create() => {
                let request = Request::post(format!("/{}", self.id))
                    .body(Ok(self.target.clone()))
                    .unwrap();

                let callback =
                    self.link
                        .callback(|response: Response<Result<String, anyhow::Error>>| {
                            let data = response.into_body();
                            Msg::ReceiveResponse(data)
                        });

                let task = yew::services::FetchService::fetch(request, callback)
                    .expect("failed to start request");

                self.fetch_task = Some(task);
                true
            }

            Msg::ReceiveResponse(response) => {
                match response {
                    Ok(msg) => {
                        self.message = Some(msg);
                    }
                    Err(error) => self.error = Some(error.to_string()),
                }
                self.fetch_task = None;
                // we want to redraw so that the page displays the location of the ISS instead of
                // 'fetching...'
                true
            }

            Msg::UpdateId(id) => {
                self.id = id;
                true
            }

            Msg::UpdateTarget(target) => {
                self.target = target;
                true
            }
        }
    }

    fn change(&mut self, _props: Self::Properties) -> ShouldRender {
        // Should only return "true" if new properties are different to
        // previously received properties.
        // This component has no properties so we will always return "false".
        false
    }

    fn view(&self) -> Html {
        html! {
            <div>
                { self.view_form() }
                { self.view_message() }
                { self.view_error() }
                { self.view_fetching_task() }
            </div>
        }
    }

    fn rendered(&mut self, _first_render: bool) {}

    fn destroy(&mut self) {}
}

#[wasm_bindgen(start)]
pub fn run_app() {
    App::<Model>::new().mount_to_body();
}
