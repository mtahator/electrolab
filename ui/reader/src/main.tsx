import "@electropaper/kit/tokens.css";
import "./reader.css";

import { applyTheme, installKeymap } from "@electropaper/kit";
import { render } from "solid-js/web";

import { App } from "./App";

applyTheme();
installKeymap();

const root = document.getElementById("root");
if (root) render(() => <App />, root);
