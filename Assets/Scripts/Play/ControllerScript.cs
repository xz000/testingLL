﻿using System.Collections;
using System.Collections.Generic;
using UnityEngine;

public class ControllerScript : MonoBehaviour
{
    public GameObject PlayerCircle;
    GameObject[] thePC;
    MoveScript[] theMS;

    public void powerrr(int PNO, List<ClickData> LCD)
    {
        foreach(ClickData CD in LCD)
        {
            switch (CD.blr)
            {
                case MButton.left:
                    break;
                case MButton.right:
                    Vector2 v2 = new Vector2(CD.xPos, CD.yPos);
                    theMS[PNO].SetTarget(v2);
                    break;
            }
        }
    }

    public void createPCs(int MaxNum)
    {
        thePC = new GameObject[MaxNum];
        theMS = new MoveScript[MaxNum];
        Vector3 v3;
        for (int i = 0; i < MaxNum; i++)
        {
            v3 = new Vector3(i * 3, 0, 0);
            thePC[i] = Instantiate(PlayerCircle, v3, Quaternion.identity);
            theMS[i] = thePC[i].GetComponent<MoveScript>();
        }
    }
}
