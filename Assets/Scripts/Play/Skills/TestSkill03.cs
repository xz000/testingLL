﻿using System.Collections;
using System.Collections.Generic;
using UnityEngine;

public class TestSkill03 : MonoBehaviour
{
    public DoSkill DS;
    public SelfExplodeScript SES;
    private float timetosing = 1;
    private float timesinged;

    // Use this for initialization
    void Start()
    {
        timesinged = 0;
    }

    public void GoTestSkill03()
    {
        if (SES.skillavaliable)
        {
            Skill();
        }
    }

    private void FixedUpdate()
    {
        if (DS.singing != 3)
        {
            timesinged = 0;
            return;
        }
        else
        {
            timesinged += Time.fixedDeltaTime;
            if (timesinged >= timetosing)
            {
                gameObject.GetComponent<SelfExplodeScript>().Skill();
                timesinged = 0;
                GetComponent<DoSkill>().singing = 0;
            }
        }
    }

    void TestSkill03SetLevel(int i)
    {
        if (i == 0)
        {
            enabled = false;
            SES.enabled = false;
        }
        else
        {
            enabled = true;
            SES.enabled = true;
        }
    }

    public void Skill()
    {
        DS.BeforeSkill();
        DS.singing = 3;
    }

    void LinkToIcon()
    {
        if (enabled)
            GameObject.Find("Canvas2").GetComponent<BottomLink>().iF.Fif = SES.CalcFA;
    }
}
