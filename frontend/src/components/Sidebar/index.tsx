'use client';

import React from 'react';
import { Settings, Mic, NotebookPen, Upload } from 'lucide-react';
import { useRouter, usePathname } from 'next/navigation';
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from '@/components/ui/tooltip';
import Info from '../Info';

const Sidebar: React.FC = () => {
  const router = useRouter();
  const pathname = usePathname();

  const isMeetingsPage = pathname === '/meetings' || pathname?.includes('/meeting-details');
  const isSettingsPage = pathname === '/settings';
  const isHomePage = pathname === '/';
  const isImportPage = pathname === '/import';

  return (
    <div className="fixed top-0 left-0 h-screen z-40">
      <div className="h-screen w-16 bg-white border-r shadow-sm flex flex-col">
        <TooltipProvider>
          <div className="flex flex-col items-center space-y-4 mt-4">
            <Tooltip>
              <TooltipTrigger asChild>
                <button
                  onClick={() => router.push('/')}
                  className={`p-2 rounded-full transition-colors duration-150 shadow-sm ${
                    isHomePage ? 'bg-red-600' : 'bg-red-500 hover:bg-red-600'
                  }`}
                >
                  <Mic className="w-5 h-5 text-white" />
                </button>
              </TooltipTrigger>
              <TooltipContent side="right">
                <p>Accueil</p>
              </TooltipContent>
            </Tooltip>

            <Tooltip>
              <TooltipTrigger asChild>
                <button
                  onClick={() => router.push('/import')}
                  className={`p-2 rounded-lg transition-colors duration-150 ${
                    isImportPage ? 'bg-gray-200' : 'hover:bg-gray-100'
                  }`}
                >
                  <Upload className="w-5 h-5 text-blue-600" />
                </button>
              </TooltipTrigger>
              <TooltipContent side="right">
                <p>Import Audio</p>
              </TooltipContent>
            </Tooltip>

            <Tooltip>
              <TooltipTrigger asChild>
                <button
                  onClick={() => router.push('/meetings')}
                  className={`p-2 rounded-lg transition-colors duration-150 ${
                    isMeetingsPage ? 'bg-gray-200' : 'hover:bg-gray-100'
                  }`}
                >
                  <NotebookPen className="w-5 h-5 text-gray-600" />
                </button>
              </TooltipTrigger>
              <TooltipContent side="right">
                <p>Meeting Notes</p>
              </TooltipContent>
            </Tooltip>

            <Tooltip>
              <TooltipTrigger asChild>
                <button
                  onClick={() => router.push('/settings')}
                  className={`p-2 rounded-lg transition-colors duration-150 ${
                    isSettingsPage ? 'bg-gray-200' : 'hover:bg-gray-100'
                  }`}
                >
                  <Settings className="w-5 h-5 text-gray-600" />
                </button>
              </TooltipTrigger>
              <TooltipContent side="right">
                <p>Settings</p>
              </TooltipContent>
            </Tooltip>

            <Info isCollapsed={true} />
          </div>
        </TooltipProvider>
      </div>
    </div>
  );
};

export default Sidebar;
