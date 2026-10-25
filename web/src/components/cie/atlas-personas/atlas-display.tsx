'use client';
import {createContext, useContext} from 'react';
/** Display density is independent of chart semantics and shared selection. */
export const AtlasDisplayContext=createContext(true);
export const useAtlasDetails=()=>useContext(AtlasDisplayContext);
