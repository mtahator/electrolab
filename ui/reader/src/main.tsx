import "@electropaper/kit/tokens.css";
import "@electropaper/kit/kit.css";
import "./reader.css";

import { Gallery, MenuHost, ToastHost, applySettings, applyTheme, installKeymap, installTooltips } from "@electropaper/kit";
import { render } from "solid-js/web";

import { App } from "./App";

applyTheme();
applySettings();
installKeymap();
installTooltips();

const root = document.getElementById("root");
// `#gallery`: the kit gallery, for reviewing every component in every theme.
const Page = location.hash === "#gallery" ? Gallery : App;
if (root)
  render(
    () => (
      <>
        <Page />
        <MenuHost />
        <ToastHost />
      </>
    ),
    root,
  );
