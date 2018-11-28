using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using UnityEngine.UI;
using System;
using System.IO;
using System.Text;

public class LSWriter : MonoBehaviour {

    public string lsFileName;
    public string lstxtPath;
    FileStream lsfs;
    StreamWriter lsSWriter;
    public float FrameLength = 1f;
    float CurrentLength = 0;
    public int FrameNum = 0;

    private void FixedUpdate()
    {
        CurrentLength += Time.fixedDeltaTime;
        while (CurrentLength >= FrameLength)
        {
            PrintList(ref ClickCatcher.LS);
            CurrentLength -= FrameLength;
            FrameNum++;
        }
    }

    private void OnEnable()
    {
        lsFileName = "/" + "LS" + string.Format("{0:D2}{1:D2}{2:D2}{3:D2}{4:D2}{5:D2}", System.DateTime.Now.Year, System.DateTime.Now.Month, System.DateTime.Now.Day, System.DateTime.Now.Hour, System.DateTime.Now.Minute, System.DateTime.Now.Second) + ".txt";
        lstxtPath = Application.dataPath + lsFileName;
        lsfs = new FileStream(lstxtPath, FileMode.OpenOrCreate, FileAccess.ReadWrite);
        lsSWriter = new StreamWriter(lsfs);
        lsSWriter.WriteLine("Started");
    }

    private void OnDisable()
    {
        lsSWriter.WriteLine("Stoped");
        lsSWriter.Close();
    }

    void PrintList(ref List<string> theLS)
    {
        /*
        while (theLS.Count != 0)
        {
            lsSWriter.WriteLine(theLS[0]);
            theLS.RemoveAt(0);
        }*/
        foreach(string str in theLS)
        {
            lsSWriter.WriteLine(str);
        }
        theLS.Clear();
    }
}
