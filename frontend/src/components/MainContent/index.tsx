'use client';

import React from 'react';

interface MainContentProps {
  children: React.ReactNode;
}

const MainContent: React.FC<MainContentProps> = ({ children }) => {
  return (
    <main className="flex-1 ml-16">
      <div className="pl-8">
        {children}
      </div>
    </main>
  );
};

export default MainContent;
