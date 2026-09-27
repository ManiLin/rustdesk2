/// <reference types="vite/client" />

import type { HTMLAttributes } from "react";

declare module "react" {
  namespace JSX {
    interface IntrinsicElements {
      "md-filled-button": HTMLAttributes<HTMLElement> & { disabled?: boolean; type?: string };
      "md-outlined-button": HTMLAttributes<HTMLElement> & { disabled?: boolean; type?: string };
      "md-text-button": HTMLAttributes<HTMLElement> & { disabled?: boolean; type?: string };
      "md-outlined-text-field": HTMLAttributes<HTMLElement> & {
        autocomplete?: string;
        label?: string;
        placeholder?: string;
        readonly?: boolean;
        required?: boolean;
        type?: string;
        value?: string;
      };
      "md-outlined-select": HTMLAttributes<HTMLElement> & { label?: string; value?: string };
      "md-select-option": HTMLAttributes<HTMLElement> & { key?: string | number; value?: string; selected?: boolean };
      "md-circular-progress": HTMLAttributes<HTMLElement> & { indeterminate?: boolean };
    }
  }
}
