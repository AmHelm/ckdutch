use anyhow::{anyhow, ensure, Context, Result};
use ckdutch_ui::{
    crypto,
    near::{Rpc, Signer},
    types::*,
};
use leptos::prelude::*;
use serde_json::json;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::{spawn_local, JsFuture};
use zeroize::Zeroizing;

#[derive(Clone, Copy, PartialEq)]
enum Page {
    Overview,
    Seal,
    Open,
    Setup,
}

#[derive(Clone, Copy)]
struct AppState {
    page: RwSignal<Page>,
    signer: RwSignal<Option<Signer>>,
    account: RwSignal<String>,
    status: RwSignal<Option<SwitchStatus>>,
    status_error: RwSignal<String>,
    busy: RwSignal<bool>,
    message: RwSignal<String>,
    error: RwSignal<bool>,
    transaction: RwSignal<String>,
    connect: RwSignal<bool>,
    now: RwSignal<f64>,
}

impl AppState {
    fn signer(self) -> Result<Signer> {
        self.signer
            .get_untracked()
            .context("Connect a testnet account first")
    }

    fn notify(self, message: &str) {
        self.message.set(message.into());
        self.error.set(false);
    }

    fn run(self, work: impl std::future::Future<Output = Result<String>> + 'static) {
        if self.busy.get_untracked() {
            return;
        }
        self.busy.set(true);
        self.transaction.set(String::new());
        self.notify("Waiting for NEAR… Keep this tab open while the transaction completes.");
        spawn_local(async move {
            match work.await {
                Ok(message) => self.notify(&message),
                Err(error) => {
                    self.error.set(true);
                    self.message.set(format!("{error:#}"));
                }
            }
            self.busy.set(false);
            self.refresh().await;
        });
    }

    async fn refresh(self) {
        let account = self.account.get_untracked();
        if account.trim().is_empty() {
            return;
        }
        let result = Rpc::default().status(&account).await;
        if self.account.get_untracked() != account {
            return;
        }
        match result {
            Ok(status) => {
                self.status.set(Some(status));
                self.status_error.set(String::new());
            }
            Err(error) => {
                self.status.set(None);
                self.status_error.set(format!("{error:#}"));
            }
        }
    }

    fn call(self, method: &'static str, args: serde_json::Value, success: &'static str) {
        let account = self.account.get_untracked();
        self.run(async move {
            let signer = self.signer()?;
            Rpc::default().status(&account).await?;
            let outcome = Rpc::default().call(&signer, &account, method, args).await?;
            self.transaction.set(outcome.hash);
            Ok(success.into())
        });
    }
}

#[component]
pub fn App() -> impl IntoView {
    let initial = web_sys::window()
        .and_then(|w| w.location().search().ok())
        .and_then(|q| web_sys::UrlSearchParams::new_with_str(&q).ok())
        .and_then(|q| q.get("account"))
        .unwrap_or_default();
    let state = AppState {
        page: RwSignal::new(Page::Overview),
        signer: RwSignal::new(None),
        account: RwSignal::new(initial),
        status: RwSignal::new(None),
        status_error: RwSignal::new(String::new()),
        busy: RwSignal::new(false),
        message: RwSignal::new(String::new()),
        error: RwSignal::new(false),
        transaction: RwSignal::new(String::new()),
        connect: RwSignal::new(false),
        now: RwSignal::new(js_sys::Date::now()),
    };
    provide_context(state);
    Effect::new(move |_| {
        state.account.track();
        state.status.set(None);
        state.status_error.set(String::new());
        spawn_local(async move {
            state.refresh().await;
        });
    });
    let clock =
        gloo_timers::callback::Interval::new(1_000, move || state.now.set(js_sys::Date::now()));
    let poll = gloo_timers::callback::Interval::new(15_000, move || {
        if !state.busy.get_untracked() {
            spawn_local(async move {
                state.refresh().await;
            });
        }
    });
    // Owner disposal drops both browser intervals on the browser thread.
    let _timers = StoredValue::new_local((clock, poll));

    view! {
        <div class="app-shell">
            <aside class="sidebar">
                <a class="brand" href="/" aria-label="CKDutch home"><span class="brand-mark" aria-hidden="true"><span class="skull-mark"/></span><span>"ckdutch"<small>"LEAVE SOMETHING BEHIND"</small></span></a>
                <div class="nav-label">"YOUR SPACE"</div>
                <nav aria-label="Main navigation">
                    <NavItem page=Page::Overview icon="grid" label="Overview"/>
                    <NavItem page=Page::Seal icon="lock" label="Seal a capsule"/>
                    <NavItem page=Page::Open icon="unlock" label="Open a capsule"/>
                    <NavItem page=Page::Setup icon="settings" label="Set up a switch"/>
                </nav>
                <div class="sidebar-bottom"><div class="little-orbit">"✳"</div><h3>"A little peace of mind."</h3><p>"For the words, memories, and things that should never be lost."</p><div class="network"><span class="dot"/>"NEAR Testnet"<span class="network-tag">"DEVELOPMENT"</span></div></div>
            </aside>
            <div class="workspace">
                <header class="topbar"><div class="breadcrumb">"Your space"<span>"/"</span>{move || match state.page.get() {Page::Overview=>"Overview",Page::Seal=>"Seal a capsule",Page::Open=>"Open a capsule",Page::Setup=>"Set up a switch"}}</div>
                    <button class="connect-button" disabled=move || state.busy.get() on:click=move |_| state.connect.set(true)><span class="wallet-dot"/>{move || state.signer.get().map(|s|s.account).unwrap_or("Connect account".into())}<Icon name="arrow"/></button>
                </header>
                <main>
                    <Show when=move || !state.message.get().is_empty()><div class="notice" class:notice-error=move || state.error.get() role="status" aria-live="polite"><span>{move || state.message.get()}</span><Show when=move || !state.transaction.get().is_empty()><a target="_blank" rel="noopener noreferrer" href=move || format!("https://testnet.nearblocks.io/txns/{}",state.transaction.get())>"View transaction ↗"</a></Show></div></Show>
                    {move || match state.page.get() {
                        Page::Overview => view!{<Overview/>}.into_any(),
                        Page::Seal => view!{<Seal/>}.into_any(),
                        Page::Open => view!{<Open/>}.into_any(),
                        Page::Setup => view!{<Setup/>}.into_any(),
                    }}
                    <footer><span>"Built for a future you can’t predict."</span><span>"Encrypted in your browser. Secured by NEAR."<Icon name="shield"/></span></footer>
                </main>
            </div>
            <Show when=move || state.connect.get()><Connect/></Show>
        </div>
    }
}

#[component]
fn NavItem(page: Page, icon: &'static str, label: &'static str) -> impl IntoView {
    let state = expect_context::<AppState>();
    view! {<button class="nav-item" class:active=move || state.page.get()==page disabled=move || state.busy.get() on:click=move |_| state.page.set(page)><Icon name=icon/><span>{label}</span><span class="nav-arrow">"↗"</span></button>}
}

#[component]
fn PageHeading(
    eyebrow: &'static str,
    title: &'static str,
    description: &'static str,
) -> impl IntoView {
    view! {<div class="page-heading"><div class="eyebrow">{eyebrow}</div><h1>{title}</h1><p>{description}</p></div>}
}

#[component]
fn AccountPicker() -> impl IntoView {
    let state = expect_context::<AppState>();
    let draft = RwSignal::new(state.account.get_untracked());
    view! {<form class="account-picker" on:submit=move |ev| {ev.prevent_default(); state.account.set(draft.get_untracked().trim().into()); if state.account.get_untracked().is_empty() {state.status.set(None);} else {spawn_local(async move{state.refresh().await;});}}>
        <label for="watch-account">"Switch account"</label><div class="input-button"><input id="watch-account" placeholder="your-switch.testnet" prop:value=move||draft.get() on:input=move|ev|draft.set(event_target_value(&ev)) disabled=move||state.busy.get()/><button class="button secondary" type="submit" disabled=move||state.busy.get()>"Look up"<Icon name="arrow"/></button></div>
    </form>}
}

#[component]
fn Overview() -> impl IntoView {
    let state = expect_context::<AppState>();
    let confirm_challenge = RwSignal::new(false);
    let status_label = move || {
        state
            .status
            .get()
            .map(|s| {
                if s.key_is_public {
                    "Key is releasable"
                } else if s.challenge_deadline_ms.is_some() {
                    "Response needed"
                } else {
                    "Standing by"
                }
            })
            .unwrap_or("No switch selected")
    };
    let can_check_in = move || {
        state
            .signer
            .get()
            .zip(state.status.get())
            .is_some_and(|(signer, status)| {
                signer.account == status.owner || status.friends.contains(&signer.account)
            })
    };
    view! {
        <PageHeading eyebrow="A LITTLE PEACE OF MIND" title="Some things should live on." description="Keep your important things safe today. Make sure they can be found tomorrow."/>
        <section class="hero-card">
            <div class="hero-copy"><div class="pill"><span class="dot"/>"YOUR LEGACY, ON YOUR TERMS"</div><h2>"A safe place for"<br/>"your just-in-case."</h2><p>"Seal a message or file. You stay in control. If you’re no longer here to check in, the people you leave behind can open it."</p><button class="button primary" disabled=move||state.busy.get() on:click=move |_|state.page.set(Page::Seal)>"Seal your first capsule"<Icon name="arrow"/></button><span class="hero-caption"><Icon name="lock"/>"Only encrypted data leaves your browser."</span></div>
            <div class="hero-art" aria-hidden="true"><div class="orbit orbit-one"/><div class="orbit orbit-two"/><div class="orbit orbit-three"/><span class="art-star star-one">"✧"</span><span class="art-star star-two">"✦"</span><div class="capsule-shadow"/><div class="envelope"><div class="envelope-paper"><span/><span/><span/></div><div class="envelope-back"/><div class="envelope-front"/><div class="envelope-seal"><span class="skull-mark"/></div></div><div class="art-label"><span class="dot"/>"For when it matters."</div></div>
        </section>
        <div class="section-title"><h2>"Your switch"</h2><span class="small-tag">"ON-CHAIN PROTECTION"</span></div>
        <section class="card switch-card"><AccountPicker/>
            <Show when=move||!state.status_error.get().is_empty()><p class="field-error">{move||state.status_error.get()}</p></Show>
            <div class="metrics"><div><span class="metric-label">"STATUS"</span><strong class="status-value"><span class="dot"/>{status_label}</strong></div><div><span class="metric-label">"RESPONSE WINDOW"</span><strong>{move||state.status.get().map(|s|duration(s.challenge_delay_ms)).unwrap_or("—".into())}</strong></div><div><span class="metric-label">"TRUSTED FRIENDS"</span><strong>{move||state.status.get().map(|s|s.friends.len().to_string()).unwrap_or("—".into())}<small>"can check in for you"</small></strong></div></div>
            <Show when=move||state.status.get().is_some_and(|s|s.challenge_deadline_ms.is_some())><div class="deadline"><Icon name="clock"/><div><strong>{move||state.status.get().and_then(|s|s.challenge_deadline_ms).map(|d|format!("Release deadline: {}",date(d))).unwrap_or_default()}</strong><p>{move||{state.now.track(); state.status.get().and_then(|s|s.challenge_deadline_ms).map(|d| {let remaining=(d as f64-state.now.get()).max(0.0) as u64; if remaining==0 {"The local countdown has ended. Chain status determines when recovery is allowed.".into()} else {format!("{} remaining to check in",duration(remaining))}}).unwrap_or_default()}}</p></div></div></Show>
            <div class="switch-bottom"><p><Icon name="shield"/>{move||if state.status.get().is_some(){"A challenge starts the clock. Check in to cancel it."}else{"Already have a switch? Enter its account above, or create one."}}</p><Show when=move||can_check_in()><button class="button primary compact" disabled=move||state.busy.get() on:click=move |_|state.call("claim_owner_is_alive",json!({}),"Check-in confirmed. The challenge is cancelled.")>"I’m still here"<Icon name="check"/></button></Show><Show when=move||state.status.get().is_none()><button class="text-button" disabled=move||state.busy.get() on:click=move |_|state.page.set(Page::Setup)>"Set up a switch ↗"</button></Show></div>
            <Show when=move||state.status.get().is_some_and(|s|!s.friends.is_empty())><div class="friends-list">"Trusted friends: "{move||state.status.get().map(|s|s.friends.join(", ")).unwrap_or_default()}</div></Show>
            <Show when=move||state.status.get().is_some()><details class="challenge-details"><summary>"Looking after someone else’s capsule?"</summary><p>"Start a challenge if the owner may no longer be here. The owner or a trusted friend can cancel it during the response window. After the deadline, anyone can request the key."</p><label class="checkbox"><input type="checkbox" prop:checked=move||confirm_challenge.get() on:change=move|ev|confirm_challenge.set(event_target_checked(&ev))/><span>"I understand this starts the public release countdown."</span></label><button class="button danger" disabled=move||state.busy.get()||!confirm_challenge.get()||state.signer.get().is_none()||state.status.get().is_none_or(|s|s.challenge_deadline_ms.is_some()) on:click=move |_|state.call("claim_owner_is_dead",json!({}),"Challenge started. The owner or a trusted friend can now respond.")>"Start a challenge"<Icon name="clock"/></button><Show when=move||state.signer.get().is_none()><p class="hint">"Connect an account to submit a challenge."</p></Show></details></Show>
        </section>
        <div class="section-title"><h2>"A simple promise. Three steps."</h2><span>"How it works"</span></div>
        <div class="steps-grid"><Step number="01" icon="lock" title="Seal something meaningful" body="A letter, a memory, a document. Encrypt it with a key derived for your switch."/><Step number="02" icon="shield" title="Stay in control" body="Share the sealed capsule anywhere. Only you can obtain its key while your switch is protected."/><Step number="03" icon="sun" title="Let it find its way" body="An unanswered challenge unlocks recovery. Anyone with the capsule can then open it."/></div>
    }
}

#[component]
fn Step(
    number: &'static str,
    icon: &'static str,
    title: &'static str,
    body: &'static str,
) -> impl IntoView {
    view! {<article class="step-card"><div class="step-top"><span class="step-icon"><Icon name=icon/></span><span>{number}</span></div><h3>{title}</h3><p>{body}</p></article>}
}

#[component]
fn Seal() -> impl IntoView {
    let state = expect_context::<AppState>();
    let note = RwSignal::new(String::new());
    let title = RwSignal::new("a-note-for-you.txt".to_string());
    let file = RwSignal::new(None::<(String, Zeroizing<Vec<u8>>)>);
    let capsule = RwSignal::new(None::<Envelope>);
    view! {
        <PageHeading eyebrow="01 / SEAL A CAPSULE" title="Give your words a tomorrow." description="Write a note or choose a file. Its contents are encrypted here, in your browser."/>
        <div class="two-column"><section class="card form-card"><div class="card-heading"><span class="step-icon"><Icon name="lock"/></span><div><h2>"What would you like to leave?"</h2><p>"A little care, tucked away for later."</p></div></div>
            <label for="capsule-name">"File name"</label><input id="capsule-name" maxlength="255" prop:value=move||title.get() on:input=move|ev|title.set(event_target_value(&ev))/>
            <label for="capsule-note">"Your message"</label><textarea id="capsule-note" rows="8" placeholder="To the people I love…" prop:value=move||note.get() on:input=move|ev|note.set(event_target_value(&ev)) disabled=move||file.get().is_some()/>
            <div class="or-divider"><span/>"OR CHOOSE A FILE"<span/></div>
            <label class="upload-zone" for="seal-file"><Icon name="upload"/><strong>{move||file.get().map(|(name,_)|name).unwrap_or("Choose something to keep safe".into())}</strong><span>"Any file · up to 10 MiB"</span><input id="seal-file" type="file" on:change=move|ev|{let selected=selected_file(&ev);spawn_local(async move{match read_file(selected, MAX_FILE_BYTES).await{Ok((name,bytes))=>{title.set(name.clone());file.set(Some((name,Zeroizing::new(bytes))));},Err(e)=>{state.error.set(true);state.message.set(e.to_string());}}});}/></label>
            <Show when=move||file.get().is_some()><button class="text-button" on:click=move |_|file.set(None)>"Remove file and write a note"</button></Show>
            <div class="form-actions"><button class="button primary" disabled=move||state.busy.get()||state.signer.get().is_none() on:click=move |_| {
                let filename=title.get_untracked(); let data=file.get_untracked().map(|(_,bytes)|bytes).unwrap_or_else(||Zeroizing::new(note.get_untracked().into_bytes()));
                capsule.set(None);
                state.run(async move {
                    ensure!(!data.is_empty(),"Write a message or select a file first");
                    ensure!(!filename.trim().is_empty(),"Give your capsule a filename");
                    let signer=state.signer()?;
                    let (key,mpc_key,hash)=Rpc::default().derive_key(&signer,&signer.account).await?;
                    let encrypted=crypto::encrypt(&key,&signer.account,&mpc_key,&filename,&data)?;
                    capsule.set(Some(encrypted)); note.set(String::new()); file.set(None);
                    state.account.set(signer.account); state.transaction.set(hash);
                    Ok("Your capsule is sealed. Download it and share the encrypted file with the people who should have it.".into())
                });
            }><Icon name="lock"/>{move||if state.busy.get(){"Sealing…"}else{"Encrypt & seal capsule"}}</button></div>
            <Show when=move||state.signer.get().is_none()><p class="hint">"Connect the account that owns your switch to seal a capsule."</p></Show>
        </section><aside class="info-column"><div class="info-card"><span class="eyebrow">"PRIVATE BY DESIGN"</span><h3>"Your browser holds the pen. And the key."</h3><p>"NEAR’s MPC network derives a key for your switch and encrypts it to a fresh public key from this browser. The response is verified before your file is encrypted."</p><div class="info-divider"/><p>"The file name and switch account are public. Your message or file contents stay encrypted."</p><p>"All capsules from the same switch use the same derived key. A release makes all of them recoverable."</p></div>
            <Show when=move||capsule.get().is_some()><div class="info-card success-card"><span class="step-icon"><Icon name="check"/></span><h3>"Sealed, and ready to share."</h3><p>"Keep the downloaded .ckdutch.json file somewhere others can find it. This app doesn’t upload or store it."</p><button class="button primary" on:click=move |_|{if let Some(c)=capsule.get_untracked(){match serde_json::to_vec_pretty(&c).map_err(anyhow::Error::from).and_then(|bytes|download(&format!("{}.ckdutch.json",c.filename),&bytes,"application/json")){Ok(())=>state.notify("Capsule download started. Share this encrypted file publicly or with your recipients."),Err(e)=>{state.error.set(true);state.message.set(e.to_string());}}}}><Icon name="download"/>"Download capsule"</button></div></Show>
        </aside></div>
    }
}

#[component]
fn Open() -> impl IntoView {
    let state = expect_context::<AppState>();
    let capsule = RwSignal::new(None::<Envelope>);
    let plaintext = RwSignal::new(None::<Zeroizing<Vec<u8>>>);
    let filename = RwSignal::new(String::new());
    view! {
        <PageHeading eyebrow="02 / OPEN A CAPSULE" title="For when the time comes." description="Choose a shared capsule to see its switch, check the release status, and recover its contents."/>
        <div class="two-column"><section class="card form-card"><label class="upload-zone large" for="open-file"><span class="step-icon"><Icon name="unlock"/></span><strong>"Choose a sealed capsule"</strong><span>"Select a .ckdutch.json file"</span><input id="open-file" type="file" accept=".json,.ckdutch.json" disabled=move||state.busy.get() on:change=move|ev|{
            let selected=selected_file(&ev); capsule.set(None);plaintext.set(None);
            spawn_local(async move{let result:Result<Envelope>=async{let (_,bytes)=read_file(selected,MAX_FILE_BYTES*2).await?;let envelope:Envelope=serde_json::from_slice(&bytes).context("This is not a valid CKDutch capsule")?;envelope.validate()?;Ok(envelope)}.await;
                match result{Ok(envelope)=>{filename.set(envelope.filename.clone());state.account.set(envelope.contract.clone());capsule.set(Some(envelope));},Err(e)=>{state.error.set(true);state.message.set(format!("{e:#}"));}}
            });
        }/></label>
        <Show when=move||capsule.get().is_some()><div class="capsule-details"><span class="eyebrow">"CAPSULE DETAILS"</span><h2>{move||filename.get()}</h2><dl><dt>"Switch account"</dt><dd>{move||capsule.get().map(|c|c.contract).unwrap_or_default()}</dd><dt>"Protection"</dt><dd>"AES-256-GCM"</dd><dt>"Release status"</dt><dd>{move||state.status.get().map(|s|if s.key_is_public{"Key is releasable"}else if s.challenge_deadline_ms.is_some(){"Challenge in progress"}else{"Protected — no active challenge"}).unwrap_or("Checking contract…")}</dd></dl></div>
            <Show when=move||!state.status_error.get().is_empty()><p class="field-error">{move||state.status_error.get()}</p></Show>
            <button class="button primary" disabled=move||state.busy.get()||state.signer.get().is_none()||state.status.get().is_none_or(|s|!s.key_is_public&&state.signer.get().is_none_or(|signer|signer.account!=s.owner)) on:click=move |_|{
                let selected=capsule.get_untracked();plaintext.set(None);
                state.run(async move{
                    let c=selected.context("Choose a capsule first")?;
                    let signer=state.signer()?;
                    let(key,mpc_key,hash)=Rpc::default().derive_key(&signer,&c.contract).await?;
                    ensure!(mpc_key==c.mpc_public_key,"The MPC domain key has changed since this capsule was sealed");
                    plaintext.set(Some(crypto::decrypt(&key,&c)?));state.transaction.set(hash);
                    Ok("Capsule opened and authenticated. Download the recovered file below.".into())
                });
            }><Icon name="unlock"/>{move||if state.busy.get(){"Recovering…"}else{"Recover key & open"}}</button>
            <p class="hint">"The owner can open a capsule at any time. Everyone else must wait for an unanswered challenge to expire and connect an account to pay transaction gas."</p>
        </Show>
        <Show when=move||plaintext.get().is_some()><div class="recovered"><h3>"Your capsule is open."</h3><button class="button primary" on:click=move |_|{if let Some(bytes)=plaintext.get_untracked(){if let Err(e)=download(&filename.get_untracked(),&bytes,"application/octet-stream"){state.error.set(true);state.message.set(e.to_string());}}}><Icon name="download"/>"Download recovered file"</button><button class="text-button" on:click=move |_|plaintext.set(None)>"Clear recovered contents"</button></div></Show>
        </section><aside class="info-column"><div class="info-card"><span class="eyebrow">"A MOMENT OF CARE"</span><h3>"There’s time to respond."</h3><p>"A capsule stays protected until someone starts a challenge and its response window passes without a check-in."</p><p>"The owner or any trusted friend can cancel a challenge. Once someone has recovered a key, a later check-in cannot take that key back."</p><button class="text-button" disabled=move||state.busy.get() on:click=move |_|state.page.set(Page::Overview)>"View switch & challenge →"</button></div></aside></div>
    }
}

#[component]
fn Setup() -> impl IntoView {
    let state = expect_context::<AppState>();
    let global = RwSignal::new(GLOBAL_CONTRACT.to_string());
    let delay = RwSignal::new("10080".to_string());
    let friends = RwSignal::new(String::new());
    let friend = RwSignal::new(String::new());
    let acknowledged = RwSignal::new(false);
    view! {
        <PageHeading eyebrow="03 / SET UP YOUR SWITCH" title="Make a plan. Find some calm." description="Your own NEAR account. Your own response window. A little preparation for the unknown."/>
        <div class="two-column"><section class="card form-card"><div class="card-heading"><span class="step-icon"><Icon name="settings"/></span><div><h2>"Create your switch"</h2><p>"Use a fresh, dedicated testnet account."</p></div></div>
            <label>"Owner account"</label><div class="readonly-field">{move||state.signer.get().map(|s|s.account).unwrap_or("Connect or create an account first".into())}</div>
            <label for="global-contract">"Global contract"</label><input id="global-contract" prop:value=move||global.get() on:input=move|ev|global.set(event_target_value(&ev))/><p class="hint">"Attach the shared contract to your account. It must include the get_status method and the deadline fix from this repository."</p>
            <label for="response-window">"Response window"</label><select id="response-window" prop:value=move||delay.get() on:change=move|ev|delay.set(event_target_value(&ev))><option value="1">"1 minute · test only"</option><option value="60">"1 hour"</option><option value="1440">"24 hours"</option><option value="10080" selected>"7 days"</option><option value="43200">"30 days"</option></select><p class="hint">"The timer starts when someone challenges, not when you create the switch."</p>
            <label for="trusted-friends">"Trusted friends "<span class="optional">"optional"</span></label><textarea id="trusted-friends" rows="3" placeholder="friend.testnet, another-friend.testnet" prop:value=move||friends.get() on:input=move|ev|friends.set(event_target_value(&ev))/><p class="hint">"These accounts can cancel a challenge on your behalf. Separate accounts with commas or new lines."</p>
            <label class="checkbox"><input type="checkbox" prop:checked=move||acknowledged.get() on:change=move|ev|acknowledged.set(event_target_checked(&ev))/><span>"I understand the global publisher can update the code, and the response window cannot be changed after setup."</span></label>
            <button class="button primary" disabled=move||state.busy.get()||state.signer.get().is_none()||!acknowledged.get() on:click=move |_|{
                let global=global.get_untracked();let delay=delay.get_untracked();let friends=friends.get_untracked();
                state.run(async move{
                    let signer=state.signer()?;
                    let friends:Result<Vec<_>>=friends.split([',','\n']).map(str::trim).filter(|s|!s.is_empty()).map(account_id).collect();
                    let outcome=Rpc::default().deploy(&signer,&global,delay.parse::<u64>()?.checked_mul(60_000).context("Window overflow")?,friends?).await?;
                    state.transaction.set(outcome.hash);state.account.set(signer.account.clone());
                    Rpc::default().status(&signer.account).await.context("Contract attached, but it lacks the required safe status API. Ask its publisher to deploy the updated contract before sealing data")?;
                    state.page.set(Page::Overview);
                    Ok("Your switch is ready. You can now seal your first capsule.".into())
                });
            }>"Create switch"<Icon name="arrow"/></button>
        </section><aside class="info-column"><div class="info-card"><span class="eyebrow">"START WITH AN ACCOUNT"</span><h3>"A home for your switch."</h3><p>"Connect an existing testnet account, or generate a dedicated subaccount using a funded account. New subaccounts receive 1 NEAR for setup and gas."</p><button class="button secondary" on:click=move |_|state.connect.set(true)>"Manage account"<Icon name="arrow"/></button></div>
            <div class="info-card"><span class="eyebrow">"ALREADY SET UP?"</span><h3>"Keep your circle up to date."</h3><p>"Connect as the owner and select your switch below to add or remove a trusted friend."</p><AccountPicker/><label for="friend-account">"Friend’s account"</label><input id="friend-account" placeholder="friend.testnet" prop:value=move||friend.get() on:input=move|ev|friend.set(event_target_value(&ev))/><div class="button-row"><button class="button secondary" disabled=move||state.busy.get()||state.signer.get().is_none_or(|s|s.account!=state.account.get()) on:click=move |_|state.call("add_friend",json!({"friend":friend.get_untracked().trim()}),"Trusted friend added.")>"Add friend"</button><button class="text-button" disabled=move||state.busy.get()||state.signer.get().is_none_or(|s|s.account!=state.account.get()) on:click=move |_|state.call("remove_friend",json!({"friend":friend.get_untracked().trim()}),"Trusted friend removed.")>"Remove"</button></div></div>
        </aside></div>
    }
}

#[component]
fn Connect() -> impl IntoView {
    let state = expect_context::<AppState>();
    let account = RwSignal::new(String::new());
    let secret = RwSignal::new(String::new());
    let label = RwSignal::new("my-switch".to_string());
    let generated = RwSignal::new(None::<Signer>);
    let backed_up = RwSignal::new(false);
    let local_error = RwSignal::new(String::new());
    let connecting = RwSignal::new(false);
    view! {
        <div class="modal-backdrop"><section class="modal" role="dialog" aria-modal="true" aria-labelledby="connect-title"><div class="modal-heading"><span class="step-icon"><Icon name="wallet"/></span><button class="close-button" aria-label="Close account dialog" disabled=move||state.busy.get()||connecting.get() on:click=move |_|state.connect.set(false)>"×"</button></div><h2 id="connect-title">"Your account, your switch."</h2><p>"Connect with a testnet full-access key. Signing happens in this browser; the key stays in memory for this session."</p>
            <Show when=move||!local_error.get().is_empty()><p class="field-error" role="alert">{move||local_error.get()}</p></Show>
            <Show when=move||state.signer.get().is_none() fallback=move||view!{
                <div class="connected-account"><span class="dot"/><strong>{move||state.signer.get().map(|s|s.account).unwrap_or_default()}</strong><button class="text-button" disabled=move||state.busy.get() on:click=move |_|{state.signer.set(None);generated.set(None);}>"Disconnect"</button></div>
                <details class="account-create"><summary>"Create a dedicated switch account"</summary><p>"Generate a new key, save its backup, then fund the account with 1 testnet NEAR from your connected account."</p>
                    <label for="subaccount-label">"Subaccount name"</label><input id="subaccount-label" prop:value=move||label.get() disabled=move||generated.get().is_some() on:input=move|ev|label.set(event_target_value(&ev))/>
                    <button class="button secondary" disabled=move||state.busy.get()||generated.get().is_some() on:click=move |_|{
                        let result:Result<Signer>=(||{let parent=state.signer()?;let label=label.get_untracked();ensure!(!label.contains('.'),"Use a single account label");Signer::generate(&format!("{}.{}",label.trim(),parent.account))})();
                        match result {Ok(signer)=>{generated.set(Some(signer));backed_up.set(false);local_error.set(String::new());},Err(e)=>local_error.set(e.to_string())}
                    }>"Generate account key"</button>
                    <Show when=move||generated.get().is_some()><p class="hint">{move||generated.get().map(|s|s.account).unwrap_or_default()}</p><button class="button secondary" on:click=move |_|{
                        if let Some(signer)=generated.get_untracked(){match signer.credentials().and_then(|c|download(&format!("{}.json",signer.account),c.as_bytes(),"application/json")){Ok(())=>{},Err(e)=>local_error.set(e.to_string())}}
                    }><Icon name="download"/>"Download private key backup"</button><label class="checkbox"><input type="checkbox" prop:checked=move||backed_up.get() on:change=move|ev|backed_up.set(event_target_checked(&ev))/><span>"I have saved the private key backup somewhere safe."</span></label>
                    <button class="button primary" disabled=move||state.busy.get()||!backed_up.get() on:click=move |_|{
                        let child=generated.get_untracked();state.run(async move{
                            let parent=state.signer()?;let child=child.context("Generate a key first")?;
                            let result=Rpc::default().create_account(&parent,&child).await?;
                            state.transaction.set(result.hash);state.account.set(child.account.clone());state.signer.set(Some(child));state.connect.set(false);state.page.set(Page::Setup);
                            Ok("Account created with 1 NEAR. Attach the global contract to finish setup.".into())
                        });
                    }>"Create account · 1 NEAR"<Icon name="arrow"/></button></Show>
                </details>
            }>
                <label for="login-account">"Account ID"</label><input id="login-account" placeholder="your-account.testnet" autocomplete="off" prop:value=move||account.get() on:input=move|ev|account.set(event_target_value(&ev))/>
                <label for="login-key">"Private key"</label><input id="login-key" type="password" autocomplete="off" placeholder="ed25519:…" prop:value=move||secret.get() on:input=move|ev|secret.set(event_target_value(&ev))/>
                <label class="credential-upload" for="credentials">"Or import a NEAR credentials JSON file"<input id="credentials" type="file" accept=".json" on:change=move|ev|{let file=selected_file(&ev);spawn_local(async move{
                    let result:Result<(String,String)>=async{let(_,bytes)=read_file(file,16*1024).await?;let bytes=Zeroizing::new(bytes);let value:serde_json::Value=serde_json::from_slice(&bytes)?;Ok((value["account_id"].as_str().context("Missing account_id")?.into(),value["private_key"].as_str().or_else(||value["secret_key"].as_str()).context("Missing private_key")?.into()))}.await;
                    match result{Ok((a,k))=>{account.set(a);secret.set(k);},Err(e)=>local_error.set(e.to_string())}
                });}/></label>
                <button class="button primary full" disabled=move||connecting.get() on:click=move |_|{
                    connecting.set(true);local_error.set(String::new());let result=Signer::import(&account.get_untracked(),&secret.get_untracked());secret.set(String::new());
                    spawn_local(async move {let result=async {let signer=result?;Rpc::default().access_key(&signer).await?;Ok::<_,anyhow::Error>(signer)}.await;
                        match result{Ok(signer)=>{state.account.set(signer.account.clone());state.signer.set(Some(signer));state.connect.set(false);state.notify("Connected. Your key is kept only for this browser session.");},Err(e)=>local_error.set(format!("{e:#}"))}connecting.set(false);
                    });
                }>{move||if connecting.get(){"Checking account…"}else{"Connect account"}}<Icon name="arrow"/></button>
                <p class="hint">"Testnet only. Use a dedicated development key. Refreshing or closing this page disconnects your account."</p>
            </Show>
        </section></div>
    }
}

fn selected_file(ev: &web_sys::Event) -> Option<web_sys::File> {
    ev.target()?
        .dyn_into::<web_sys::HtmlInputElement>()
        .ok()?
        .files()?
        .get(0)
}

async fn read_file(file: Option<web_sys::File>, limit: usize) -> Result<(String, Vec<u8>)> {
    let file = file.context("Choose a file first")?;
    ensure!(file.size() <= limit as f64, "File is too large");
    let buffer = JsFuture::from(file.array_buffer())
        .await
        .map_err(|_| anyhow!("Could not read file"))?;
    Ok((file.name(), js_sys::Uint8Array::new(&buffer).to_vec()))
}

fn download(name: &str, bytes: &[u8], mime: &str) -> Result<()> {
    let array = js_sys::Array::new();
    array.push(&js_sys::Uint8Array::from(bytes));
    let options = web_sys::BlobPropertyBag::new();
    options.set_type(mime);
    let blob = web_sys::Blob::new_with_u8_array_sequence_and_options(&array, &options)
        .map_err(|_| anyhow!("Could not prepare download"))?;
    let url = web_sys::Url::create_object_url_with_blob(&blob)
        .map_err(|_| anyhow!("Could not prepare download URL"))?;
    let window = web_sys::window().context("No browser window")?;
    let document = window.document().context("No document")?;
    let anchor = document
        .create_element("a")
        .map_err(|_| anyhow!("Could not prepare download link"))?
        .dyn_into::<web_sys::HtmlAnchorElement>()
        .map_err(|_| anyhow!("Invalid download link"))?;
    anchor.set_href(&url);
    let safe_name = name
        .chars()
        .map(|c| {
            if c == '/' || c == '\\' || c.is_control() {
                '_'
            } else {
                c
            }
        })
        .collect::<String>();
    anchor.set_download(&safe_name);
    anchor.click();
    gloo_timers::callback::Timeout::new(30_000, move || {
        let _ = web_sys::Url::revoke_object_url(&url);
    })
    .forget();
    Ok(())
}

fn duration(ms: u64) -> String {
    let seconds = ms.div_ceil(1_000);
    if seconds >= 86_400 {
        format!("{}d {}h", seconds / 86_400, (seconds % 86_400) / 3_600)
    } else if seconds >= 3_600 {
        format!("{}h {}m", seconds / 3_600, (seconds % 3_600) / 60)
    } else if seconds >= 60 {
        format!("{}m {}s", seconds / 60, seconds % 60)
    } else {
        format!("{seconds}s")
    }
}

fn date(ms: u64) -> String {
    js_sys::Date::new(&JsValue::from_f64(ms as f64))
        .to_utc_string()
        .into()
}

#[component]
fn Icon(name: &'static str) -> impl IntoView {
    let path=match name {
        "sail"=>"M12 3v13H3L12 3ZM14 6l7 10h-7V6ZM3 19h18l-4 3H7l-4-3Z",
        "grid"=>"M3 3h7v7H3V3Zm11 0h7v7h-7V3ZM3 14h7v7H3v-7Zm11 0h7v7h-7v-7Z",
        "lock"=>"M6 10V7a6 6 0 0 1 12 0v3M5 10h14v11H5V10Zm7 5v2",
        "unlock"=>"M7 10V7a5 5 0 0 1 9-3M5 10h14v11H5V10Zm7 5v2",
        "shield"=>"M12 3 3 6v6c0 5 9 9 9 9s9-4 9-9V6l-9-3Zm-4 9 3 3 5-6",
        "arrow"=>"M4 12h16m-6-6 6 6-6 6",
        "clock"=>"M12 7v5l3 2M22 12a10 10 0 1 1-20 0 10 10 0 0 1 20 0",
        "check"=>"m5 12 4 4L19 6",
        "sun"=>"M12 2v2m0 16v2M2 12h2m16 0h2M5 5l2 2m10 10 2 2M5 19l2-2M17 7l2-2M17 12a5 5 0 1 1-10 0 5 5 0 0 1 10 0",
        "upload"=>"M12 16V3m-5 5 5-5 5 5M4 16v5h16v-5",
        "download"=>"M12 3v13m-5-5 5 5 5-5M4 16v5h16v-5",
        "wallet"=>"M3 5h16v4H3V5Zm0 4v12h18V9H3Zm12 4h6v4h-6v-4",
        _=>"M4 7h16M4 17h16M8 4v6m8 4v6",
    };
    view! {<svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d=path/></svg>}
}
