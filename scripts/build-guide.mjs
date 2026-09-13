import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { meta, sections } from "../docs/guide/content.mjs";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const locales = ["zh-CN", "en", "ja"];
const images = {};
for (const section of sections) {
  if (!section.image || images[section.image]) continue;
  images[section.image] = {};
  for (const locale of locales) {
    const file = path.join(root, "docs/guide/images", `${section.image}-${locale}.png`);
    if (fs.existsSync(file)) {
      const bytes = fs.readFileSync(file);
      images[section.image][locale] = {
        src: "data:image/png;base64," + bytes.toString("base64"),
        width: bytes.readUInt32BE(16),
        height: bytes.readUInt32BE(20),
      };
    }
  }
}
for (const section of sections) {
  if (section.title.length !== 3 || section.steps.some((row) => row.length !== 3 || row.some((s) => !s.trim()))) throw new Error(`Incomplete translation: ${section.id}`);
}
if (process.argv.includes("--check")) {
  for (const section of sections.filter((s) => s.image)) {
    if (!Object.keys(images[section.image]).length) throw new Error(`Missing illustration: ${section.image}`);
  }
}
const data = JSON.stringify({ meta, sections, images }).replaceAll("<", "\\u003c");
const html = `<!doctype html>
<html lang="zh-CN"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<meta name="color-scheme" content="light dark"><title>VTB Toolkit · Guide</title>
<style>
:root{font-family:Inter,"Segoe UI","Microsoft YaHei","Noto Sans JP",system-ui,sans-serif;color:#e8edf5;background:#0d1420;line-height:1.75;--muted:#a1b0c7;--line:#304056;--card:#152132;--accent:#7cd8d0;scroll-behavior:smooth;color-scheme:dark}*{box-sizing:border-box}body{margin:0}button,input,select{font:inherit}button,select{cursor:pointer;padding:.45rem .8rem;border:1px solid var(--line);border-radius:8px;background:var(--card);color:inherit}button:hover,a:hover{color:var(--accent)}button:focus-visible,select:focus-visible,input:focus-visible,a:focus-visible{outline:3px solid var(--accent);outline-offset:3px}header{position:sticky;top:0;background:#0d1420f2;backdrop-filter:blur(12px);z-index:2;border-bottom:1px solid var(--line)}.bar{max-width:1500px;margin:auto;display:flex;align-items:center;gap:14px;padding:12px 30px;flex-wrap:wrap}.brand{letter-spacing:.08em;font-size:.9rem;margin-right:auto}.brand span{color:var(--accent)}.layout{max-width:1500px;margin:auto;display:grid;grid-template-columns:260px minmax(0,1fr);gap:50px;padding:34px 30px}aside{position:sticky;top:105px;align-self:start;max-height:calc(100vh - 125px);overflow:auto}aside h2{font-size:.8rem;color:var(--accent);letter-spacing:.1em}#search{width:100%;padding:9px;background:var(--card);border:1px solid var(--line);border-radius:7px;color:inherit}nav a{display:block;text-decoration:none;color:var(--muted);font-size:.85rem;padding:6px 2px;border-bottom:1px solid #263143}nav a b{color:var(--accent);margin-right:9px;font-weight:500}h1{font-size:clamp(2rem,4.2vw,3.7rem);line-height:1.15;letter-spacing:-.04em;margin:20px 0}h2{line-height:1.3}p{margin:12px 0}.kicker{font-size:.8rem;color:var(--accent);letter-spacing:.14em}.subtitle{font-size:1.35rem;color:#c0cde0}.muted,figcaption{color:var(--muted);font-size:.85rem}.flow{display:flex;gap:7px;margin:28px 0 45px;flex-wrap:wrap}.flow span{background:var(--card);padding:10px 14px;border:1px solid var(--line);border-radius:8px;font-size:.9rem}.flow span:not(:last-child):after{content:' →';color:var(--accent)}section{padding:30px 0;border-top:1px solid var(--line);scroll-margin-top:100px}section h2{font-size:1.55rem;margin:0 0 25px}section h2 span{font-size:.9rem;color:var(--accent);font-weight:400;display:block;margin-bottom:8px}ol.steps{padding:0;list-style:none;counter-reset:step}ol.steps li{counter-increment:step;position:relative;padding:2px 0 2px 48px;margin:18px 0}ol.steps li:before{content:counter(step,decimal-leading-zero);position:absolute;left:0;top:3px;font-size:.8rem;color:var(--accent);border:1px solid var(--line);border-radius:50%;width:29px;height:29px;text-align:center;line-height:27px}figure{margin:28px 0;background:var(--card);border:1px solid var(--line);border-radius:13px;overflow:hidden}figure button{display:block;border:0;border-radius:0;padding:0;width:100%;background:none}figure img{width:100%;height:auto;max-height:750px;object-fit:contain;display:block}figcaption{padding:12px 18px}dialog{width:min(1500px,98vw);max-width:98vw;max-height:97vh;padding:12px;background:#0d1420;border:1px solid var(--line);color:inherit;border-radius:12px}dialog::backdrop{background:#000d}dialog img{max-width:100%;height:auto;display:block;margin:10px auto}dialog form{text-align:right;position:sticky;top:0;background:#0d1420;z-index:1}footer{border-top:1px solid var(--line);padding:28px 0;color:var(--muted);font-size:.85rem}#empty{padding:30px;color:var(--muted)}@media(max-width:900px){.layout{display:block;padding:24px 18px}.bar{padding:10px 18px}aside{position:static;max-height:none;margin-bottom:28px}nav{display:grid;grid-template-columns:1fr 1fr;gap:0 15px}h1{font-size:2.5rem}}@media(prefers-reduced-motion:reduce){:root{scroll-behavior:auto}}@media print{:root{background:#fff;color:#111;color-scheme:light;--muted:#555;--line:#ccc;--card:#fff;--accent:#087568}header,aside,#empty,dialog{display:none!important}.layout{display:block;padding:0;max-width:none}h1{font-size:32pt}section{break-inside:auto;scroll-margin:0}h2{break-after:avoid}figure{break-inside:avoid}figure img{max-height:16cm}.flow span{color:#111}footer{color:#555}section[hidden]{display:block}body{font-size:10pt}.subtitle{color:#333}}
nav a[hidden]{display:none}
</style></head><body>
<header><div class="bar"><strong class="brand">VTB <span>TOOLKIT</span> / FIELD GUIDE</strong><select id="language" aria-label="Language / 语言 / 言語"><option value="zh-CN">简体中文</option><option value="en">English</option><option value="ja">日本語</option></select><button id="print"></button></div></header>
<div class="layout"><aside><h2 id="contents"></h2><input id="search" type="search"><nav id="toc" aria-label="Contents"></nav></aside><main><div class="kicker">LIVE → RECORD → CREATE</div><h1 id="title"></h1><p class="subtitle" id="subtitle"></p><p id="intro"></p><p class="muted" id="version"></p><div class="flow" id="flow"></div><div id="sections"></div><p id="empty" hidden></p><footer>VTB Toolkit · Bilibili / YouTube · 2026</footer></main></div>
<dialog id="zoom"><form method="dialog"><button id="close"></button></form><img id="zoom-image" alt=""></dialog>
<script id="guide-data" type="application/json">${data}</script>
<script>
const data=JSON.parse(document.getElementById('guide-data').textContent),locales=['zh-CN','en','ja'];
const requested=new URLSearchParams(location.search).get('lang')||navigator.language;
let lang=/^en(?:-|$)/i.test(requested)?'en':/^ja(?:-|$)/i.test(requested)?'ja':'zh-CN';
const el=id=>document.getElementById(id), text=(id,value)=>{el(id).textContent=value};
function render(){const i=locales.indexOf(lang);document.documentElement.lang=lang;document.title=data.meta.title[i];el('language').value=lang;
for(const key of ['title','subtitle','intro','version','print','empty','close'])text(key,data.meta[key][i]);text('contents',data.meta.toc[i]);el('search').placeholder=data.meta.search[i];el('search').setAttribute('aria-label',data.meta.search[i]);el('toc').setAttribute('aria-label',data.meta.toc[i]);
el('flow').replaceChildren(...data.meta.flow.map(row=>{const node=document.createElement('span');node.textContent=row[i];return node}));el('toc').replaceChildren();el('sections').replaceChildren();
data.sections.forEach((item,n)=>{const section=document.createElement('section');section.id=item.id;const heading=document.createElement('h2'),tag=document.createElement('span');tag.textContent=String(n+1).padStart(2,'0')+' / VTB TOOLKIT';heading.append(tag,document.createTextNode(item.title[i]));section.append(heading);
const list=document.createElement('ol');list.className='steps';item.steps.forEach(row=>{const li=document.createElement('li');li.textContent=row[i];list.append(li)});section.append(list);
const shots=data.images[item.image]||{},shotLocale=shots[lang]?lang:Object.keys(shots)[0];if(shotLocale){const figure=document.createElement('figure'),button=document.createElement('button'),img=document.createElement('img'),caption=document.createElement('figcaption');img.src=shots[shotLocale].src;img.width=shots[shotLocale].width;img.height=shots[shotLocale].height;img.alt=item.title[i]+' · '+shotLocale;img.loading='lazy';button.setAttribute('aria-label',data.meta.shot[i]);button.append(img);button.onclick=()=>{el('zoom-image').src=img.src;el('zoom-image').alt=img.alt;el('zoom').showModal()};caption.textContent=String(n+1).padStart(2,'0')+' · '+item.title[i]+' · '+data.meta.shot[i]+' ('+shotLocale+')'+(['review','cover'].includes(item.image)?' · '+data.meta.demo[i]:'');figure.append(button,caption);section.append(figure)}
el('sections').append(section);const link=document.createElement('a'),num=document.createElement('b');num.textContent=String(n+1).padStart(2,'0');link.href='#'+item.id;link.append(num,document.createTextNode(item.title[i]));el('toc').append(link)});filter();}
function filter(){const query=el('search').value.toLocaleLowerCase(lang),items=[...el('sections').children];items.forEach((s,n)=>{s.hidden=!s.textContent.toLocaleLowerCase(lang).includes(query);el('toc').children[n].hidden=s.hidden});el('empty').hidden=items.some(s=>!s.hidden)}
el('language').onchange=e=>{lang=e.target.value;try{const url=new URL(location.href);url.searchParams.set('lang',lang);history.replaceState(null,'',url)}catch{}render()};el('search').oninput=filter;el('print').onclick=()=>window.print();el('zoom').onclick=e=>{if(e.target===el('zoom'))el('zoom').close()};render();
</script></body></html>`;
for (const output of ["docs/user-guide.html", "public/guide/index.html"]) {
  const file = path.join(root, output); fs.mkdirSync(path.dirname(file), { recursive: true }); fs.writeFileSync(file, html);
}
for (const [i, locale] of locales.entries()) {
  let markdown = `# ${meta.title[i]}\n\n${meta.subtitle[i]}\n\n${meta.intro[i]}\n\n${meta.version[i]}\n\n`;
  for (const section of sections) {
    markdown += `## ${section.title[i]}\n\n` + section.steps.map((s, n) => `${n + 1}. ${s[i]}`).join("\n\n") + "\n\n";
    const available = Object.keys(images[section.image] ?? {});
    if (available.length) {
      const shotLocale = available.includes(locale) ? locale : available[0];
      markdown += `![${section.title[i]}](guide/images/${section.image}-${shotLocale}.png)\n\n`;
      markdown += `${meta.shot[i]} (${shotLocale})${["review", "cover"].includes(section.image) ? ` · ${meta.demo[i]}` : ""}\n\n`;
    }
  }
  fs.writeFileSync(path.join(root, `docs/user-guide.${locale}.md`), markdown.trimEnd() + "\n");
}
console.log(`Guide: ${sections.length} sections × 3 languages; ${Object.values(images).reduce((n, shots) => n + Object.keys(shots).length, 0)} screenshots; ${(Buffer.byteLength(html)/1048576).toFixed(1)} MiB offline HTML`);
